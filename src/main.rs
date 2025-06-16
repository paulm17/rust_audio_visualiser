use iced::{
  Background, Color, Element, Length, Task as Command,
  widget::{Canvas, button, canvas, column, row},
};
use rodio::{Decoder, OutputStream, Sink, Source};
use rustfft::{FftPlanner, num_complex::Complex};
use std::fs::File;
use std::io::BufReader;
use std::{
  collections::VecDeque,
  sync::{Arc, Mutex},
  thread,
  time::Duration,
};

mod components;
use crate::components::{tap::Tap, visualiser::VisualizerCanvas};

const DEFAULT_NUM_BARS: usize = 75;
const DEFAULT_BAR_WIDTH: f32 = 8.0;
const DEFAULT_STARTING_ANGLE: f32 = 0.0;
const MIN_BAR_HEIGHT: f32 = 4.0;
const MIN_DECIBEL: f32 = -90.0;
const MAX_DECIBEL: f32 = -10.0;
// const SAMPLE_RATE: usize = 44100;
const BUFFER_SIZE: usize = 2048;
const UPDATE_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Debug, Clone)]
pub enum Message {
  LoadFile,
  Play,
  Pause,
  Stop,
  Tick,
  AudioData(Vec<f32>),
}

pub struct AudioVisualizer {
  is_playing: bool,
  is_loaded: bool,
  is_decaying: bool,
  audio_data: Arc<Mutex<VecDeque<f32>>>,
  frequency_data: Vec<f32>,
  sink: Option<Sink>,
  _stream: Option<OutputStream>,
  file_path: Option<String>,
  canvas_cache: canvas::Cache,
  tap_sender: Arc<Mutex<Option<std::sync::mpsc::Sender<Vec<f32>>>>>,
  audio_receiver: Option<std::sync::mpsc::Receiver<Vec<f32>>>,
}

impl AudioVisualizer {
  fn new() -> (Self, Command<Message>) {
    (Self::default(), Command::none())
  }

  fn title(&self) -> String {
    String::from("Rust Audio Visualizer")
  }

  fn load_audio_file(&mut self) {
    if let Some(path) = &self.file_path {
      // Open audio output
      match OutputStream::try_default() {
        Ok((stream, stream_handle)) => {
          // Create a sink attached to the stream handle
          if let Ok(sink) = Sink::try_new(&stream_handle) {
            // Open and decode the file
            if let Ok(file) = File::open(path) {
              if let Ok(decoder) = Decoder::new(BufReader::new(file)) {
                // Set up our channel for tapping
                let (sender, receiver) = std::sync::mpsc::channel();
                *self.tap_sender.lock().unwrap() = Some(sender.clone());
                self.audio_receiver = Some(receiver);

                // Convert samples to f32
                let f32_source = decoder.convert_samples::<f32>();

                // Wrap in our Tap adapter, which implements rodio::Source
                let tapped = Tap::new(f32_source, sender);

                // Append to sink (playback) and start paused
                sink.append(tapped);
                sink.pause();

                // Store the sink and stream so they live as long as we need
                self.sink = Some(sink);
                self._stream = Some(stream);
                self.is_loaded = true;

                // Kick off the FFT thread
                self.start_audio_analysis();
              }
            }
          }
        }
        Err(e) => {
          eprintln!("Failed to create audio stream: {}", e);
        }
      }
    }
  }

  fn start_audio_analysis(&mut self) {
    // If we have a receiver, spin up the analysis thread
    if let Some(receiver) = self.audio_receiver.take() {
      // Clone for thread
      let audio_data = self.audio_data.clone();

      // Plan the FFT up front to avoid reallocating on every chunk
      let mut planner = FftPlanner::new();
      let fft = planner.plan_fft_forward(BUFFER_SIZE);

      thread::spawn(move || {
        while let Ok(samples) = receiver.recv() {
          if samples.len() >= BUFFER_SIZE {
            // Build the complex buffer once per chunk
            let mut buffer: Vec<Complex<f32>> =
              samples[..BUFFER_SIZE].iter().map(|&x| Complex::new(x, 0.0)).collect();

            // Run the FFT
            fft.process(&mut buffer);

            // Convert to frequency magnitudes
            let magnitudes: Vec<f32> =
              buffer.iter().take(BUFFER_SIZE / 2).map(|c| c.norm()).collect();

            // Push into our shared audio_data for the UI thread
            if let Ok(mut data_buffer) = audio_data.lock() {
              data_buffer.clear();
              data_buffer.extend(magnitudes);
            }
          }
        }
      });
    }
  }

  fn update_frequency_data(&mut self, magnitudes: Vec<f32>) {
    // Group frequencies into bars for visualization
    // self.frequency_data = self.group_frequencies_into_bars(magnitudes);

    let new_bars = self.group_frequencies_into_bars(magnitudes);
    // exponential smoothing factor (0.0 = no smoothing, 1.0 = freeze)
    const SMOOTHING: f32 = 0.2;
    for (old, new) in self.frequency_data.iter_mut().zip(new_bars.iter()) {
      *old = *old * SMOOTHING + *new * (1.0 - SMOOTHING);
    }

    self.canvas_cache.clear();
  }

  fn group_frequencies_into_bars(&self, magnitudes: Vec<f32>) -> Vec<f32> {
    let total_bins = magnitudes.len();
    let half_bars = (DEFAULT_NUM_BARS + 1) / 2; // For mirroring
    let interval = total_bins / half_bars;
    let fft_size = BUFFER_SIZE as f32;
    let max_index = half_bars; // This creates the mirroring effect

    (0..DEFAULT_NUM_BARS)
      .map(|i| {
        // Mirror logic: use modulo to create symmetric pattern
        let idx = ((i % max_index) * interval).min(total_bins - 1);
        let raw = magnitudes[idx] / fft_size;
        let db = if raw > 0.0 {
          (20.0 * raw.log10()).clamp(MIN_DECIBEL, MAX_DECIBEL)
        } else {
          MIN_DECIBEL
        };
        let h = map_range(db, MIN_DECIBEL, MAX_DECIBEL, MIN_BAR_HEIGHT, 150.0);
        h.max(MIN_BAR_HEIGHT)
      })
      .collect()
  }

  fn update(&mut self, message: Message) -> Command<Message> {
    match message {
      Message::LoadFile => {
        if let Some(path) =
          rfd::FileDialog::new().add_filter("Audio", &["mp3", "wav", "flac", "ogg"]).pick_file()
        {
          self.file_path = Some(path.to_string_lossy().to_string());
          self.load_audio_file();
        }
        Command::none()
      }
      Message::Play => {
        if self.sink.is_none() {
          if let Some(_) = &self.file_path {
            self.load_audio_file();
          }
        }
        if let Some(sink) = &self.sink {
          sink.play();
          self.is_playing = true;
          self.is_decaying = false;
        }
        Command::none()
      }
      Message::Pause => {
        if let Some(sink) = &self.sink {
          sink.pause();
          self.is_playing = false;
          self.is_decaying = true;
        }
        Command::none()
      }
      Message::Stop => {
        // Tear down the current sink (drains the queue)
        if let Some(sink) = &self.sink {
          sink.stop();
        }
        self.is_playing = false;
        self.is_decaying = true;
        // And immediately rebuild it (paused at start)
        if let Some(_) = &self.file_path {
          self.load_audio_file();
        }
        Command::none()
      }
      Message::AudioData(data) => {
        self.update_frequency_data(data);
        self.canvas_cache.clear();
        Command::none()
      }
      Message::Tick => {
        if self.is_playing {
          // scope the lock so it’s dropped before we call update_frequency_data
          let maybe_mags = {
            let mut guard = self.audio_data.lock().unwrap();
            if !guard.is_empty() {
              // drain into a fresh Vec and drop the lock
              Some(guard.drain(..).collect::<Vec<f32>>())
            } else {
              None
            }
          };

          if let Some(mags) = maybe_mags {
            self.update_frequency_data(mags);
          }
        } else if self.is_decaying {
          let mut any_above = false;
          for h in &mut self.frequency_data {
            let new_h = (*h - 8.0).max(MIN_BAR_HEIGHT);
            if new_h > MIN_BAR_HEIGHT {
              any_above = true;
            }
            *h = new_h;
          }
          if !any_above {
            self.is_decaying = false;
          }
        }

        Command::none()
      }
    }
  }

  fn view(&self) -> Element<Message> {
    let btn_loadfile_color = if !self.is_loaded {
      // Not loaded: blue
      Color::parse("#1447e6").unwrap()
    } else {
      // Loaded: gray
      Color::parse("#99a1af").unwrap()
    };

    let btn_play_color = if !self.is_loaded {
      // Not loaded: gray
      Color::parse("#99a1af").unwrap()
    } else if self.is_playing {
      // Playing: gray
      Color::parse("#99a1af").unwrap()
    } else {
      // Loaded but not playing: green
      Color::parse("#007a55").unwrap()
    };

    let btn_pause_color = if !self.is_loaded {
      // Not loaded: gray
      Color::parse("#99a1af").unwrap()
    } else if self.is_playing {
      // Playing: blue
      Color::parse("#1447e6").unwrap()
    } else {
      // Loaded but not playing: gray
      Color::parse("#99a1af").unwrap()
    };

    let btn_stop_color = if !self.is_loaded {
      // Not loaded: gray
      Color::parse("#99a1af").unwrap()
    } else if self.is_playing {
      // Playing: blue
      Color::parse("#1447e6").unwrap()
    } else {
      // Loaded but not playing: gray
      Color::parse("#99a1af").unwrap()
    };

    let controls = row![
      button("Load File").on_press(Message::LoadFile).style(move |_, _| {
        button::Style {
          background: Some(Background::Color(btn_loadfile_color)),
          ..button::Style::default()
        }
      }),
      button("Play").on_press(Message::Play).style(move |_, _| {
        button::Style {
          background: Some(Background::Color(btn_play_color)),
          ..button::Style::default()
        }
      }),
      button("Pause").on_press(Message::Pause).style(move |_, _| {
        button::Style {
          background: Some(Background::Color(btn_pause_color)),
          ..button::Style::default()
        }
      }),
      button("Stop").on_press(Message::Stop).style(move |_, _| {
        button::Style {
          background: Some(Background::Color(btn_stop_color)),
          ..button::Style::default()
        }
      }),
    ]
    .spacing(10);

    let visualizer = Canvas::new(VisualizerCanvas {
      frequency_data: &self.frequency_data,
      cache: &self.canvas_cache,
    })
    .width(Length::Fill)
    .height(Length::Fill);

    column![controls, visualizer].spacing(20).padding(20).into()
  }

  fn subscription(&self) -> iced::Subscription<Message> {
    if self.is_playing || self.is_decaying {
      iced::time::every(UPDATE_INTERVAL).map(|_| Message::Tick)
    } else {
      iced::Subscription::none()
    }
  }
}

impl Default for AudioVisualizer {
  fn default() -> Self {
    Self {
      is_playing: false,
      is_loaded: false,
      is_decaying: false,
      audio_data: Arc::new(Mutex::new(VecDeque::new())),
      frequency_data: vec![MIN_BAR_HEIGHT; DEFAULT_NUM_BARS],
      sink: None,
      _stream: None,
      file_path: None,
      canvas_cache: canvas::Cache::default(),
      tap_sender: Arc::new(Mutex::new(None)),
      audio_receiver: None,
    }
  }
}

fn map_range(value: f32, from_min: f32, from_max: f32, to_min: f32, to_max: f32) -> f32 {
  let from_range = from_max - from_min;
  let to_range = to_max - to_min;
  let scaled = (value - from_min) / from_range;
  to_min + scaled * to_range
}

fn main() -> iced::Result {
  iced::application(AudioVisualizer::title, AudioVisualizer::update, AudioVisualizer::view)
    .subscription(AudioVisualizer::subscription)
    .run_with(AudioVisualizer::new)
}
