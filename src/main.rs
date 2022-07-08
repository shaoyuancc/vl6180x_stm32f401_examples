//! Prints "Hello, world!" on the host console using semihosting

#![no_main]
#![no_std]

use panic_semihosting as _;

use crate::hal::{pac, prelude::*};
use cortex_m_rt::entry;
use cortex_m_semihosting::hprintln;
use stm32f4xx_hal as hal;

#[entry]
fn main() -> ! {
    let _ = 5;
    hprintln!("Hello, world!").unwrap();

    if let (Some(dp), Some(cp)) = (
        pac::Peripherals::take(),
        cortex_m::peripheral::Peripherals::take(),
    ) {
        // Set up the LED. On the Black Pill it's connected to pin PC13.
        let gpioc = dp.GPIOC.split();
        let mut led = gpioc.pc13.into_push_pull_output();

        // Set up User button. On the Black Pill it's connected to pin PA0
        let gpioa = dp.GPIOA.split();
        let user_button = gpioa.pa0.into_pull_up_input();

        // Set up the system clock. We want to run at 48MHz for this one.
        let rcc = dp.RCC.constrain();
        let clocks = rcc.cfgr.sysclk(48.MHz()).freeze();

        // Create a delay abstraction based on SysTick
        let mut delay = cp.SYST.delay(&clocks);

        let mut state = State::LedOff;

        loop {
            match state {
                State::LedOn => led.set_low(),
                State::LedOff => led.set_high(),
            }

            if user_button.is_low() {
                state.cycle();
            }
            delay.delay_ms(1_u32);
        }
    }

    loop {}
}
enum State {
    LedOn,
    LedOff,
}

impl State {
    fn cycle(&mut self) {
        use State::*;
        *self = match *self {
            LedOn => LedOff,
            LedOff => LedOn,
        }
    }
}
