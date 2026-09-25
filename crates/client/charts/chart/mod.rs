//! One module per chart: an iced canvas that draws what `model/` computed.

pub mod ladder;
pub mod timeandsales;

/// What a chart's canvas asks of its pane.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Message {
    /// In pixels, as the chart's model scrolls.
    Scrolled(f32),
    ResetScroll,
}
