use core::fmt::Debug;

use embedded_graphics::mono_font::ascii::FONT_6X10;
use embedded_graphics::mono_font::{MonoTextStyle, MonoTextStyleBuilder};
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::DrawTarget;
use embedded_hal::blocking::i2c::Write;
use embedded_text::alignment::HorizontalAlignment;
use embedded_text::style::{HeightMode, TextBoxStyle, TextBoxStyleBuilder};
use heapless::String;
use ssd1306::mode::BufferedGraphicsMode;
use ssd1306::prelude::{DisplayConfig, I2CInterface};
use ssd1306::rotation::DisplayRotation;
use ssd1306::size::{DisplaySize, DisplaySize128x32};
use ssd1306::{I2CDisplayInterface, Ssd1306};

use crate::on_update_default;

use rumcake::display::drivers::DisplayDriver;
use rumcake::display::DisplayDevice;

pub mod driver {
    pub use ssd1306::*;
}

pub static DEFAULT_STYLE: MonoTextStyle<'_, BinaryColor> = MonoTextStyleBuilder::new()
    .font(&FONT_6X10)
    .text_color(BinaryColor::On)
    .build();

pub static DEFAULT_TEXTBOX_STYLE: TextBoxStyle = TextBoxStyleBuilder::new()
    .height_mode(HeightMode::FitToText)
    .alignment(HorizontalAlignment::Left)
    .build();

pub static DEFAULT_HEADER_STYLE: TextBoxStyle = TextBoxStyleBuilder::new()
    .height_mode(HeightMode::FitToText)
    .alignment(HorizontalAlignment::Center)
    .build();

pub trait Ssd1306I2cDisplayDriver<S: DisplaySize = DisplaySize128x32>: DisplayDevice {
    const SIZE: S;
    const ROTATION: DisplayRotation = DisplayRotation::Rotate90;

    fn setup_i2c() -> impl Write<Error = impl Debug>;

    /// Update the SSD1306 screen. The frame buffer gets cleared before this function is called.
    /// After this function is called, the display will be flushed. So, an implementor simply
    /// needs to create the graphics to display on the screen, and does not need to clear the
    /// frame buffer or flush the data to the screen.
    async fn on_update(
        display: &mut Ssd1306<
            I2CInterface<impl Write<Error = impl Debug>>,
            S,
            BufferedGraphicsMode<S>,
        >,
    ) {
        match Self::ROTATION {
            DisplayRotation::Rotate0 | DisplayRotation::Rotate180 => {
                on_update_default!(display, "HORIZONTAL", 12);
            }
            DisplayRotation::Rotate90 | DisplayRotation::Rotate270 => {
                on_update_default!(display, "VERTICAL", 8);
            }
        }
    }
}

pub async fn setup_display_driver<S: DisplaySize, K: Ssd1306I2cDisplayDriver<S>>(
    _k: K,
) -> impl DisplayDriver<K> {
    let mut display = Ssd1306::new(
        I2CDisplayInterface::new(K::setup_i2c()),
        K::SIZE,
        K::ROTATION,
    )
    .into_buffered_graphics_mode();
    display.init().unwrap();

    RumcakeSsd1306I2cDisplayDriver { driver: display }
}

pub struct RumcakeSsd1306I2cDisplayDriver<E: Debug, DI: Write<Error = E>, S: DisplaySize> {
    driver: Ssd1306<I2CInterface<DI>, S, BufferedGraphicsMode<S>>,
}

impl<E: Debug, DI: Write<Error = E>, S: DisplaySize, K: Ssd1306I2cDisplayDriver<S>> DisplayDriver<K>
    for RumcakeSsd1306I2cDisplayDriver<E, DI, S>
{
    async fn on_update(&mut self) {
        self.driver.clear(BinaryColor::Off).unwrap();
        K::on_update(&mut self.driver).await;
        self.driver.flush().unwrap();
    }

    async fn turn_off(&mut self) {
        self.driver.set_display_on(false).unwrap();
    }

    async fn turn_on(&mut self) {
        self.driver.set_display_on(true).unwrap();
    }
}
