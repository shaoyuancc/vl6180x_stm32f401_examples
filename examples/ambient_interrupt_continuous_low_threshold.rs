#![no_main]
#![no_std]

use panic_semihosting as _;
use rtic::app;

#[app(device = hal::pac, peripherals = true)]
mod app {
    use cortex_m_semihosting::hprintln;
    use hal::prelude::*;
    use stm32f4xx_hal as hal;

    type I2cType = hal::i2c::I2c<
        you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml::I2C1,
        (
            hal::gpio::gpiob::PB8<hal::gpio::Alternate<4, hal::gpio::OpenDrain>>,
            hal::gpio::gpiob::PB9<hal::gpio::Alternate<4, hal::gpio::OpenDrain>>,
        ),
    >;

    type Vl6180xType = vl6180x::VL6180X<vl6180x::AmbientContinuousMode, I2cType>;

    type Tof1Type = vl6180x::VL6180XwPins<
        vl6180x::AmbientContinuousMode,
        I2cType,
        hal::gpio::gpiob::PB7<hal::gpio::Output>,
        hal::gpio::gpiob::PB6<hal::gpio::Input>,
    >;

    #[shared]
    struct Shared {
        led: hal::gpio::gpioc::PC13<hal::gpio::Output<hal::gpio::PushPull>>,
        tof_1: Tof1Type,
    }

    #[local]
    struct Local {}

    #[init]
    fn init(ctx: init::Context) -> (Shared, Local, init::Monotonics) {
        let dp = ctx.device;
        let cp = ctx.core;
        let rcc = dp.RCC.constrain();
        let clocks = rcc.cfgr.sysclk(48.MHz()).freeze();
        let mut delay = cp.SYST.delay(&clocks);
        let mut exti = dp.EXTI;
        let mut syscfg = dp.SYSCFG.constrain();

        // Set up led
        let gpioc = dp.GPIOC.split();
        let mut led = gpioc.pc13.into_push_pull_output();
        led.set_high();

        // Set up I2C
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
        let i2c: I2cType = dp.I2C1.i2c((scl, sda), 400.kHz(), &clocks);

        // Set up vl6180x
        let mut x_shutdown_pin = gpiob.pb7.into_push_pull_output();
        x_shutdown_pin.set_high();

        // Ensure vl6180x is booted before trying to communicate with it
        delay.delay_ms(2_u8);

        let mut interrupt_pin = gpiob.pb6.into_pull_up_input();
        interrupt_pin.make_interrupt_source(&mut syscfg);
        interrupt_pin.trigger_on_edge(&mut exti, hal::gpio::Edge::Rising);
        interrupt_pin.enable_interrupt(&mut exti);

        let mut tof_config = vl6180x::Config::new();
        tof_config.set_ambient_interrupt_mode(vl6180x::AmbientInterruptMode::LevelLow);
        tof_config.set_ambient_low_interrupt_threshold(40);
        tof_config.set_ambient_analogue_gain_level(7).expect("saag");
        tof_config.set_ambient_result_scaler(15).expect("sas");
        let vl6180x: Vl6180xType = vl6180x::VL6180X::with_config(i2c, &tof_config)
            .expect("vl")
            .start_ambient_continuous_mode()
            .expect("ct");

        let tof_1: Tof1Type = vl6180x::VL6180XwPins {
            vl6180x,
            x_shutdown_pin,
            interrupt_pin,
        };

        (Shared { led, tof_1 }, Local {}, init::Monotonics())
    }

    #[task(binds=EXTI9_5, shared = [led, tof_1])]
    fn exti95_event(ctx: exti95_event::Context) {
        let led = ctx.shared.led;
        let tof_1 = ctx.shared.tof_1;

        hprintln!("-------- Interrupt! --------").unwrap();
        (led, tof_1).lock(|led, tof_1| {
            led.set_low();
            match tof_1.vl6180x.read_ambient() {
                Ok(raw) => hprintln!("Ambient Read: {}", raw).unwrap(),
                Err(e) => hprintln!("Error {:?}", e).unwrap(),
            };
            led.set_high();
            tof_1.interrupt_pin.clear_interrupt_pending_bit();
            tof_1.vl6180x.clear_all_interrupts().expect("clrall");
        });
    }

    #[idle()]
    fn idle(_ctx: idle::Context) -> ! {
        loop {}
    }
}
