use std::sync::mpsc::Sender;

use rodio::Source;

use crate::BUFFER_SIZE;

/// A `Source` wrapper that forwards every sample to the sender in
/// fixed‐size chunks, then plays the sample through unchanged.
pub struct Tap<S>
where
  S: Source<Item = f32>,
{
  inner: S,
  buf: Vec<f32>,
  sender: Sender<Vec<f32>>,
}

impl<S> Tap<S>
where
  S: Source<Item = f32>,
{
  pub fn new(source: S, sender: Sender<Vec<f32>>) -> Self {
    Tap { inner: source, buf: Vec::with_capacity(BUFFER_SIZE), sender }
  }
}

impl<S> Iterator for Tap<S>
where
  S: Source<Item = f32>,
{
  type Item = f32;

  fn next(&mut self) -> Option<f32> {
    // Pull the next sample from the inner source
    if let Some(sample) = self.inner.next() {
      self.buf.push(sample);
      if self.buf.len() >= BUFFER_SIZE {
        // Send the chunk off to your FFT thread
        let full = std::mem::take(&mut self.buf);
        let _ = self.sender.send(full);
        self.buf = Vec::with_capacity(BUFFER_SIZE);
      }
      Some(sample)
    } else {
      None
    }
  }
}

impl<S> Source for Tap<S>
where
  S: Source<Item = f32>,
{
  #[inline]
  fn current_frame_len(&self) -> Option<usize> {
    self.inner.current_frame_len()
  }
  #[inline]
  fn channels(&self) -> u16 {
    self.inner.channels()
  }
  #[inline]
  fn sample_rate(&self) -> u32 {
    self.inner.sample_rate()
  }
  #[inline]
  fn total_duration(&self) -> Option<std::time::Duration> {
    self.inner.total_duration()
  }
}
