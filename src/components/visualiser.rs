use iced::{
  Color, Point, Rectangle, Theme,
  widget::canvas::{self, Geometry, Path},
};

use crate::{DEFAULT_BAR_WIDTH, DEFAULT_NUM_BARS, DEFAULT_STARTING_ANGLE, MIN_BAR_HEIGHT, Message};

pub struct VisualizerCanvas<'a> {
  pub frequency_data: &'a [f32],
  pub cache: &'a canvas::Cache,
}

impl<'a> canvas::Program<Message> for VisualizerCanvas<'a> {
  type State = ();

  fn draw(
    &self,
    _state: &Self::State,
    renderer: &iced::Renderer,
    _theme: &Theme,
    bounds: Rectangle,
    _cursor: iced::mouse::Cursor,
  ) -> Vec<Geometry> {
    let geometry = self.cache.draw(renderer, bounds.size(), |frame| {
      let center = Point::new(bounds.width * 0.5, bounds.height * 0.5);
      let radius = (bounds.width * bounds.width + bounds.height * bounds.height).sqrt() / 8.0;
      let angle_interval = 2.0 * std::f32::consts::PI / DEFAULT_NUM_BARS as f32;
      let max_bar_height = bounds.width.min(bounds.height) / 2.0 - radius;

      // Draw circular bars similar to the React version
      for (i, &height) in self.frequency_data.iter().enumerate() {
        // always draw every bar from the ring, capping at max_bar_height
        let bar_height = height.min(max_bar_height);
        let angle = (i as f32 * angle_interval) + DEFAULT_STARTING_ANGLE;

        let inner_x = center.x + radius * angle.cos();
        let inner_y = center.y + radius * angle.sin();
        // outer is simply radius + bar_height
        let outer_x = center.x + (radius + bar_height) * angle.cos();
        let outer_y = center.y + (radius + bar_height) * angle.sin();

        // Create a rectangular bar
        let bar_path = Path::new(|builder| {
          // Perpendicular angle for bar width (subtract 90 degrees like React)
          let perpendicular_angle = angle - std::f32::consts::PI / 2.0;
          let half_width = (DEFAULT_BAR_WIDTH * 1.2) / 2.0;

          let dx = half_width * perpendicular_angle.cos();
          let dy = half_width * perpendicular_angle.sin();

          builder.move_to(Point::new(inner_x - dx, inner_y - dy));
          builder.line_to(Point::new(inner_x + dx, inner_y + dy));
          builder.line_to(Point::new(outer_x + dx, outer_y + dy));
          builder.line_to(Point::new(outer_x - dx, outer_y - dy));
          builder.close();
        });

        // Color based on frequency intensity - more vibrant like the React version
        let intensity = (bar_height - MIN_BAR_HEIGHT) / (max_bar_height - MIN_BAR_HEIGHT);
        let color = Color::from_rgb(
          0.9 + intensity * 0.1, // Higher base red for more magenta
          0.3 + intensity * 0.4, // Lower green component
          0.9 + intensity * 0.1, // Higher base blue for more magenta
        );

        frame.fill(&bar_path, color);
      }
    });

    vec![geometry]
  }
}
