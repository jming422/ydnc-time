use tracing::warn;
use tui::{
    style::Color,
    widgets::canvas::{Context, Line},
};

pub struct Donut {
    /// The bigger this divisor, the smaller the donut hole.
    hole_divisor: f64,
    /// The bigger this divisor, the bigger the margins will be. Divisor=1 for no margin.
    margin_divisor: f64,
    slices: Vec<(u8, Color)>,
}

impl Donut {
    pub fn new(hole_divisor: f64, margin_divisor: f64, slices: Vec<(u8, Color)>) -> Self {
        let sum: u8 = slices.iter().map(|t| t.0).sum();
        if sum > 100 {
            warn!("Creating donut chart with over 100% filled; it will loop back around to 0% and overwrite stuff!");
        }

        Self {
            hole_divisor,
            margin_divisor,
            slices,
        }
    }

    /// Returns a function suitable for passing to canvas::Canvas::paint
    pub fn painter(self) -> impl Fn(&mut Context<'_>) {
        move |ctx: &mut Context| {
            // We will be drawing "rays" to make a circle. Each ray begins at
            // the circle's origin, and extends to (cos(a), sin(a)) where a is
            // the ray's angle. The more rays we draw, the more "filled in" the
            // colors look on larger displays, but the more work the program has
            // to do during drawing. Also, much of this work will likely be
            // useless at smaller resolutions, since it will end up drawing over
            // itself. I did a bunch of testing at various chart sizes, and
            // landed on 360 rays having my favorite appearance.
            let num_rays = 360;

            // Also, especially at lower resolutions, sometimes the rays at the
            // "end" will overlap those at the "beginning." Draw in reverse
            // (clockwise) so that the rays at the "beginning" of the chart are
            // favored over the ones at the "end" in the event that there would
            // be any overdrawing (happens at small resolutions).
            let mut slice_idx = self.slices.len() - 1;
            let mut last_slice_perc = 1.0;
            for ray in (0..num_rays).rev() {
                // Drawing in reverse means this starts at 100 and goes down to 0
                let percentage = (ray as f64) / (num_rays as f64);

                // The breakpoint where we'll switch to the previous (b/c reverse) color
                let slice_beg_perc = last_slice_perc - ((self.slices[slice_idx].0 as f64) / 100.0);

                // Check if we need to switch colors
                let color = if percentage <= slice_beg_perc {
                    if slice_idx == 0 {
                        // We hit bottom, stop drawing
                        Color::Reset
                    } else {
                        slice_idx -= 1;
                        last_slice_perc = percentage;
                        self.slices.get(slice_idx).map_or(Color::Reset, |t| t.1)
                    }
                } else {
                    self.slices.get(slice_idx).map_or(Color::Reset, |t| t.1)
                };

                let angle = std::f64::consts::TAU * percentage;
                let x = angle.cos();
                let y = angle.sin();
                ctx.draw(&Line {
                    x1: x / self.hole_divisor, // Origin
                    y1: y / self.hole_divisor,
                    x2: x / self.margin_divisor, // Margin
                    y2: y / self.margin_divisor,
                    color,
                });
            }
        }
    }
}
