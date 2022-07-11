#![allow(clippy::empty_loop)]
#![no_std]
#![no_main]

use cortex_m_rt::ExceptionFrame;
use cortex_m_rt::{entry, exception};
use cortex_m_semihosting::hprintln;
use stm32f4xx_hal as hal;

use hal::gpio::{Output, Pin};

use panic_semihosting as _;
use vl6180x;

use hal::{pac, prelude::*};

#[entry]
fn main() -> ! {
    if let (Some(dp), Some(cp)) = (
        pac::Peripherals::take(),
        cortex_m::peripheral::Peripherals::take(),
    ) {
        // Set up the system clock. We want to run at 48MHz for this one.
        let rcc = dp.RCC.constrain();
        let clocks = rcc.cfgr.sysclk(48.MHz()).freeze();

        // Create a delay abstraction based on SysTick
        let mut delay = cp.SYST.delay(&clocks);

        // Set up the LED. On the Black Pill it's connected to pin PC13.
        let gpioc = dp.GPIOC.split();
        let mut led = gpioc.pc13.into_push_pull_output();
        led.set_high();

        // Set up I2C - SCL is PB8 and SDA is PB9; they are set to Alternate Function 4
        let gpiob = dp.GPIOB.split();
        let scl = gpiob
            .pb8
            .into_alternate()
            .internal_pull_up(true)
            .set_open_drain();
        let sda = gpiob
            .pb9
            .into_alternate()
            .internal_pull_up(true)
            .set_open_drain();
        let i2c = dp.I2C1.i2c((scl, sda), 400.kHz(), &clocks);

        // Set up TOF distance sensor
        let mut tof_config = vl6180x::Config::new();
        tof_config.set_ambient_analogue_gain_level(7).expect("saag");
        tof_config.set_ambient_result_scaler(15).expect("sas");

        // To create sensor with default configuration:
        let mut tof_1 = vl6180x::VL6180X::with_config(i2c, &tof_config)
            .expect("vl")
            .into_dynamic_mode();

        // Set up button
        let gpioa = dp.GPIOA.split();
        let btn = gpioa.pa0.into_pull_up_input();

        // Set up XShut pin
        let mut xshut: Pin<'B', 7, Output> = gpiob.pb7.into_push_pull_output();

        led.set_low();
        tof_1.try_power_off(&mut xshut).expect("power off");
        delay.delay_ms(2000_u32);
        led.set_high();
        tof_1.try_power_on_and_init(&mut xshut).expect("power on");

        tof_1.try_change_i2c_address(20).expect("change address");

        // Set up state for the loop
        let mut state = State::RangeSinglePoll;
        let mut was_pressed = btn.is_low();

        // This runs continuously, as fast as possible
        loop {
            let is_pressed = btn.is_low();
            if !was_pressed && is_pressed {
                use State::*;
                // On exiting state
                match state {
                    RangeContinuousPoll => tof_1
                        .try_stop_range_continuous_mode()
                        .expect("stop range continuous"),
                    AmbientContinuousPoll => tof_1
                        .try_stop_ambient_continuous_mode()
                        .expect("stop ambeint continuous"),
                    _ => (),
                };

                state.cycle();
                was_pressed = true;

                // On first entering state
                match state {
                    WelcomeText => hprintln!("{}", WELCOME_TEXT).unwrap(),
                    RangeContinuousPoll => tof_1
                        .try_start_range_continuous_mode()
                        .expect("start range cont"),
                    AmbientContinuousPoll => tof_1
                        .try_start_ambient_continuous_mode()
                        .expect("start ambient cont"),
                    EndText => hprintln!("{}", END_TEXT).unwrap(),
                    _ => (),
                };
            } else if !is_pressed {
                was_pressed = false;
            }
            // While in state
            use State::*;
            match state {
                RangeContinuousPoll => match tof_1.try_read_range_mm_blocking() {
                    Ok(range) => hprintln!("Range Continuous Poll: {}mm", range).unwrap(),
                    Err(e) => {
                        hprintln!("Error reading TOF sensor Continuous Poll! {:?}", e).unwrap()
                    }
                },
                RangeSinglePoll => match tof_1.try_poll_range_mm_single_blocking() {
                    Ok(range) => hprintln!("Range Single Poll: {}mm", range).unwrap(),
                    Err(e) => hprintln!("Error reading TOF sensor Single Poll! {:?}", e).unwrap(),
                },
                AmbientContinuousPoll => {
                    match tof_1.try_read_ambient_lux_blocking() {
                        Ok(ambient) => {
                            hprintln!("Ambient Continuous Poll: {:08.4}lux", ambient).unwrap()
                        }
                        Err(e) => {
                            hprintln!("Error reading TOF sensor Ambient Continuous Poll! {:?}", e)
                                .unwrap()
                        }
                    };
                    delay.delay_ms(500_u32);
                }
                AmbientSinglePoll => {
                    match tof_1.try_poll_ambient_lux_single_blocking() {
                        Ok(ambient) => {
                            hprintln!("Ambient Single Poll: {:08.4}lux", ambient).unwrap()
                        }
                        Err(e) => {
                            hprintln!("Error reading TOF sensor Ambient Single Poll! {:?}", e)
                                .unwrap()
                        }
                    };
                }
                _ => (),
            };
        }
    }

    loop {}
}

const WELCOME_TEXT: &str = "VL6180X\nDynamic Mode Single Sensor Test Suite";
const END_TEXT: &str = "Goodbye\nSee you soon!";

enum State {
    WelcomeText,
    RangeContinuousPoll,
    RangeSinglePoll,
    AmbientContinuousPoll,
    AmbientSinglePoll,
    EndText,
}

impl State {
    fn cycle(&mut self) {
        use State::*;
        *self = match *self {
            WelcomeText => RangeContinuousPoll,
            RangeContinuousPoll => RangeSinglePoll,
            RangeSinglePoll => AmbientContinuousPoll,
            AmbientContinuousPoll => AmbientSinglePoll,
            AmbientSinglePoll => EndText,
            EndText => WelcomeText,
        }
    }
}

#[exception]
unsafe fn HardFault(ef: &ExceptionFrame) -> ! {
    panic!("{:#?}", ef);
}
