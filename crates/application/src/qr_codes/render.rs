use std::str::FromStr;

/// Формат картинки кода.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageFormat {
    Svg,
    Png,
}

impl ImageFormat {
    pub fn content_type(self) -> &'static str {
        match self {
            ImageFormat::Svg => "image/svg+xml",
            ImageFormat::Png => "image/png",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Svg => "svg",
            ImageFormat::Png => "png",
        }
    }
}

impl FromStr for ImageFormat {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "svg" => Ok(ImageFormat::Svg),
            "png" => Ok(ImageFormat::Png),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedImage {
    pub format: ImageFormat,
    pub bytes: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
#[error("failed to render QR code: {0}")]
pub struct RenderError(pub String);

/// Рисует QR с заданным содержимым. Реализация — в `template-infrastructure`.
pub trait QrRenderer: Send + Sync {
    /// `size` — минимальная сторона картинки в пикселях (для SVG — в единицах viewBox).
    fn render(
        &self,
        content: &str,
        format: ImageFormat,
        size: u32,
    ) -> Result<RenderedImage, RenderError>;
}
