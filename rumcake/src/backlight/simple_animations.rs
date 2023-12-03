use super::drivers::SimpleBacklightDriver;
use super::BacklightDevice;
use crate::math::sin;
use crate::{Cycle, LEDEffect};
use postcard::experimental::max_size::MaxSize;
use rumcake_macros::{generate_items_from_enum_variants, Cycle, LEDEffect};

use core::marker::PhantomData;
use core::u8;
use defmt::{error, warn, Debug2Format};
use keyberon::layout::Event;
use num_derive::FromPrimitive;
use rand::rngs::SmallRng;
use rand_core::SeedableRng;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, MaxSize)]
pub struct BacklightConfig {
    pub enabled: bool,
    pub effect: BacklightEffect,
    pub val: u8,
    pub speed: u8,
}

impl BacklightConfig {
    pub const fn default() -> Self {
        BacklightConfig {
            enabled: true,
            effect: BacklightEffect::Solid,
            val: 255,
            speed: 86,
        }
    }
}

impl Default for BacklightConfig {
    fn default() -> Self {
        Self::default()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy)]
pub enum BacklightCommand {
    Toggle,
    TurnOn,
    TurnOff,
    NextEffect,
    PrevEffect,
    SetEffect(BacklightEffect),
    SetValue(u8),
    IncreaseValue(u8),
    DecreaseValue(u8),
    SetSpeed(u8),
    IncreaseSpeed(u8),
    DecreaseSpeed(u8),
    #[cfg(feature = "storage")]
    SaveConfig,
    ResetTime, // normally used internally for syncing LEDs for split keyboards
}

#[generate_items_from_enum_variants("const {variant_shouty_snake_case}_ENABLED: bool = true")]
#[derive(
    FromPrimitive,
    Serialize,
    Deserialize,
    Debug,
    Clone,
    Copy,
    LEDEffect,
    Cycle,
    PartialEq,
    Eq,
    MaxSize,
)]
pub enum BacklightEffect {
    Solid,

    #[animated]
    Breathing,

    #[animated]
    #[reactive]
    Reactive,
}

impl BacklightEffect {
    pub(crate) fn is_enabled<D: BacklightDevice>(&self) -> bool {
        match self {
            BacklightEffect::Solid => D::SOLID_ENABLED,
            BacklightEffect::Breathing => D::BREATHING_ENABLED,
            BacklightEffect::Reactive => D::REACTIVE_ENABLED,
        }
    }
}

pub(super) struct BacklightAnimator<K: BacklightDevice, D: SimpleBacklightDriver<K>> {
    pub(super) config: BacklightConfig,
    pub(super) buf: u8, // Stores the current brightness/value. Different from `self.config.val`.
    pub(super) time_of_last_press: u32,
    pub(super) tick: u32,
    pub(super) driver: D,
    pub(super) rng: SmallRng,
    pub(super) phantom: PhantomData<K>,
}

impl<K: BacklightDevice, D: SimpleBacklightDriver<K>> BacklightAnimator<K, D> {
    pub fn new(config: BacklightConfig, driver: D) -> Self {
        Self {
            config,
            tick: 0,
            driver,
            buf: 0,
            time_of_last_press: 0,
            rng: SmallRng::seed_from_u64(1337),
            phantom: PhantomData,
        }
    }

    pub async fn turn_on(&mut self) {
        if let Err(err) = self.driver.turn_on().await {
            warn!("[BACKLIGHT] Animations have been enabled, but the backlight LEDs could not be turned on: {}", Debug2Format(&err));
        };
    }

    pub async fn turn_off(&mut self) {
        if let Err(err) = self.driver.turn_off().await {
            warn!("[BACKLIGHT] Animations have been disabled, but the backlight LEDs could not be turned off: {}", Debug2Format(&err));
        };
    }

    pub async fn process_command(&mut self, command: BacklightCommand) {
        match command {
            BacklightCommand::Toggle => {
                self.config.enabled = !self.config.enabled;
            }
            BacklightCommand::TurnOn => {
                self.config.enabled = true;
            }
            BacklightCommand::TurnOff => {
                self.config.enabled = false;
            }
            BacklightCommand::NextEffect => {
                while {
                    self.config.effect.increment();
                    self.config.effect.is_enabled::<K>()
                } {}
            }
            BacklightCommand::PrevEffect => {
                while {
                    self.config.effect.decrement();
                    self.config.effect.is_enabled::<K>()
                } {}
            }
            BacklightCommand::SetEffect(effect) => {
                self.config.effect = effect;
            }
            BacklightCommand::SetValue(val) => {
                self.config.val = val;
            }
            BacklightCommand::IncreaseValue(amount) => {
                self.config.val = self.config.val.saturating_add(amount);
            }
            BacklightCommand::DecreaseValue(amount) => {
                self.config.val = self.config.val.saturating_sub(amount);
            }
            BacklightCommand::SetSpeed(speed) => {
                self.config.speed = speed;
            }
            BacklightCommand::IncreaseSpeed(amount) => {
                self.config.speed = self.config.speed.saturating_add(amount);
            }
            BacklightCommand::DecreaseSpeed(amount) => {
                self.config.speed = self.config.speed.saturating_sub(amount);
            }
            #[cfg(feature = "storage")]
            BacklightCommand::SaveConfig => {
                super::storage::BACKLIGHT_SAVE_SIGNAL.signal(());
            }
            BacklightCommand::ResetTime => {
                self.tick = 0;
            }
        }
    }

    pub fn set_brightness(&mut self, calc: impl Fn(&mut Self, f32) -> u8) {
        let seconds = (self.tick as f32 / K::FPS as f32)
            * (self.config.speed as f32 * 1.5 / u8::MAX as f32 + 0.5);
        self.buf = (calc(self, seconds) as u16 * self.config.val as u16 / u8::MAX as u16) as u8
    }

    pub fn register_event(&mut self, event: Event) {
        match event {
            Event::Press(row, col) => {
                self.time_of_last_press = self.tick;
            }
            Event::Release(_row, _col) => {} // nothing for now. maybe change some effects to behave depending on the state of a key.
        }
    }

    pub async fn tick(&mut self) {
        if !self.config.enabled {
            return;
        }

        match self.config.effect {
            BacklightEffect::Solid => {
                if K::SOLID_ENABLED {
                    self.set_brightness(|_animator, _time| u8::MAX)
                }
            }
            BacklightEffect::Breathing => {
                if K::BREATHING_ENABLED {
                    self.set_brightness(|_animator, time| {
                        ((sin(time) + 1.0) * u8::MAX as f32 / 2.0) as u8
                    })
                }
            }
            BacklightEffect::Reactive => {
                if K::REACTIVE_ENABLED {
                    self.set_brightness(|animator, _time| {
                        // Base speed: LED fades after one second
                        let pos = (((animator.tick - animator.time_of_last_press) as f32
                            / K::FPS as f32)
                            * (animator.config.speed as f32 * 1.5 / u8::MAX as f32 + 0.5))
                            .min(1.0);
                        u8::MAX - (u8::MAX as f32 * pos) as u8
                    })
                }
            }
        }

        if let Err(err) = self.driver.write(self.buf).await {
            error!(
                "[BACKLIGHT] Couldn't update backlight: {}",
                Debug2Format(&err)
            );
        };

        self.tick += 1;
    }
}
