//! Картинки QR-кодов (крейт `qrcode`): SVG и PNG.

use std::io::Cursor;

use image::Luma;
use qrcode::render::svg;
use qrcode::{EcLevel, QrCode};
use template_application::qr_codes::{ImageFormat, QrRenderer, RenderError, RenderedImage};

/// Уровень коррекции M (~15%): печатные коды на кампусе переживают царапины и блики,
/// а модулей ещё немного — код читается с расстояния.
pub struct QrCodeRenderer;

impl QrRenderer for QrCodeRenderer {
    fn render(
        &self,
        content: &str,
        format: ImageFormat,
        size: u32,
    ) -> Result<RenderedImage, RenderError> {
        let code = QrCode::with_error_correction_level(content, EcLevel::M)
            .map_err(|error| RenderError(error.to_string()))?;
        let bytes = match format {
            ImageFormat::Svg => code
                .render::<svg::Color<'_>>()
                .min_dimensions(size, size)
                .dark_color(svg::Color("#000000"))
                .light_color(svg::Color("#ffffff"))
                .build()
                .into_bytes(),
            ImageFormat::Png => {
                let image = code.render::<Luma<u8>>().min_dimensions(size, size).build();
                let mut bytes = Vec::new();
                image
                    .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
                    .map_err(|error| RenderError(error.to_string()))?;
                bytes
            }
        };
        Ok(RenderedImage { format, bytes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_png_and_svg() {
        let png = QrCodeRenderer
            .render("https://qr.example/s/abcdefghijkl", ImageFormat::Png, 256)
            .unwrap();
        assert!(png.bytes.starts_with(b"\x89PNG"));

        let svg = QrCodeRenderer
            .render("https://qr.example/s/abcdefghijkl", ImageFormat::Svg, 256)
            .unwrap();
        assert!(String::from_utf8(svg.bytes).unwrap().contains("<svg"));
    }
}
