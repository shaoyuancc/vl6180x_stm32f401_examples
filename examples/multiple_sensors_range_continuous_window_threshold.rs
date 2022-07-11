#![no_main]
#![no_std]

use panic_semihosting as _;
use rtic::app;

#[app(device = hal::pac, peripherals = true)]
mod app {
    use cortex_m_semihosting::hprintln;
    use hal::prelude::*;
    use stm32f4xx_hal as hal;

    type I2c = hal::i2c::I2c<
        you_must_enable_the_rt_feature_for_the_pac_in_your_cargo_toml::I2C1,
        (
            hal::gpio::gpiob::PB8<hal::gpio::Alternate<4, hal::gpio::OpenDrain>>,
            hal::gpio::gpiob::PB9<hal::gpio::Alternate<4, hal::gpio::OpenDrain>>,
        ),
    >;
    type I2cProxy = shared_bus::I2cProxy<'static, shared_bus::AtomicCheckMutex<I2c>>;

    type Vl6180xType = vl6180x::VL6180X<vl6180x::RangeContinuousMode, I2cProxy>;

    type Tof1Type = vl6180x::VL6180XwPins<
        vl6180x::RangeContinuousMode,
        I2cProxy,
        hal::gpio::gpiob::PB2<hal::gpio::Output>,
        hal::gpio::gpiob::PB1<hal::gpio::Input>,
    >;

    type Tof2Type = vl6180x::VL6180XwPins<
        vl6180x::RangeContinuousMode,
        I2cProxy,
        hal::gpio::gpioa::PA3<hal::gpio::Output>,
        hal::gpio::gpioa::PA2<hal::gpio::Input>,
    >;

    pub struct I2cDevices {
        tof_1: Tof1Type,
        tof_2: Tof2Type,
    }

    #[shared]
    struct Shared {
        i2c_devices: I2cDevices,
        led: hal::gpio::gpioc::PC13<hal::gpio::Output<hal::gpio::PushPull>>,
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

        let gpioa = dp.GPIOA.split();
        let gpiob = dp.GPIOB.split();
        let gpioc = dp.GPIOC.split();

        // Set up led
        let mut led = gpioc.pc13.into_push_pull_output();
        led.set_high();

        // Create the shared-bus I2C manager.
        let bus_manager: &'static _ = {
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

            shared_bus::new_atomic_check!(I2c = i2c).unwrap()
        };

        let mut tof_config = vl6180x::Config::new();
        tof_config.set_range_interrupt_mode(vl6180x::RangeInterruptMode::LevelHigh);
        tof_config.set_range_high_interrupt_threshold(20);

        // Set up x_shut pins
        let mut x_shut_1 = gpiob.pb2.into_push_pull_output();
        x_shut_1.set_high();

        let mut x_shut_2 = gpioa.pa3.into_push_pull_output();
        x_shut_2.set_high();

        delay.delay_ms(2_u8);

        // Set up interrupt pins
        let mut int_1 = gpiob.pb1.into_pull_up_input();
        int_1.make_interrupt_source(&mut syscfg);
        int_1.trigger_on_edge(&mut exti, hal::gpio::Edge::Rising);
        int_1.enable_interrupt(&mut exti);

        let mut int_2 = gpioa.pa2.into_pull_up_input();
        int_2.make_interrupt_source(&mut syscfg);
        int_2.trigger_on_edge(&mut exti, hal::gpio::Edge::Rising);
        int_2.enable_interrupt(&mut exti);

        // Set up vl6180x's
        let vl6180x_1 =
            vl6180x::VL6180X::with_config(bus_manager.acquire_i2c(), &tof_config).expect("vl1");
        let vl6180x_1 = vl6180x_1.power_off(&mut x_shut_1).expect("pof1");
        let vl6180x_2 =
            vl6180x::VL6180X::with_config(bus_manager.acquire_i2c(), &tof_config).expect("vl2");
        let vl6180x_2 = vl6180x_2.power_off(&mut x_shut_2).expect("pof2");

        // Turn them on one by one and set their addresses
        let mut vl6180x_1 = vl6180x_1.power_on_and_init(&mut x_shut_1).expect("pon1");
        vl6180x_1.change_i2c_address(10).expect("sa1");
        let vl6180x_1: Vl6180xType = vl6180x_1.start_range_continuous_mode().expect("ct1");

        let mut vl6180x_2 = vl6180x_2.power_on_and_init(&mut x_shut_2).expect("pon2");
        vl6180x_2.change_i2c_address(11).expect("sa2");
        let vl6180x_2: Vl6180xType = vl6180x_2.start_range_continuous_mode().expect("ct2");

        let tof_1: Tof1Type = vl6180x::VL6180XwPins {
            vl6180x: vl6180x_1,
            x_shutdown_pin: x_shut_1,
            interrupt_pin: int_1,
        };

        let tof_2: Tof2Type = vl6180x::VL6180XwPins {
            vl6180x: vl6180x_2,
            x_shutdown_pin: x_shut_2,
            interrupt_pin: int_2,
        };

        let i2c_devices = I2cDevices { tof_1, tof_2 };

        (Shared { i2c_devices, led }, Local {}, init::Monotonics())
    }

    #[task(binds=EXTI1, shared = [led, i2c_devices])]
    fn exti1_event(ctx: exti1_event::Context) {
        let led = ctx.shared.led;
        let i2c_devices = ctx.shared.i2c_devices;

        hprintln!("-------- Interrupt! -------- (tof_1)").unwrap();
        (led, i2c_devices).lock(|led, i2c_devices| {
            led.set_low();

            match i2c_devices.tof_1.vl6180x.read_range_mm() {
                Ok(range) => hprintln!("Range Read: {}mm", range).unwrap(),
                Err(e) => hprintln!("Error {:?}", e).unwrap(),
            };
            led.set_high();
            i2c_devices
                .tof_1
                .interrupt_pin
                .clear_interrupt_pending_bit();
            i2c_devices
                .tof_1
                .vl6180x
                .clear_all_interrupts()
                .expect("clrall");
        });
    }

    #[task(binds=EXTI2, shared = [led, i2c_devices])]
    fn exti2_event(ctx: exti2_event::Context) {
        let led = ctx.shared.led;
        let i2c_devices = ctx.shared.i2c_devices;

        hprintln!("-------- Interrupt! -------- (tof_2)").unwrap();
        (led, i2c_devices).lock(|led, i2c_devices| {
            led.set_low();

            match i2c_devices.tof_2.vl6180x.read_range_mm() {
                Ok(range) => hprintln!("Range Read: {}mm", range).unwrap(),
                Err(e) => hprintln!("Error {:?}", e).unwrap(),
            };
            led.set_high();
            i2c_devices
                .tof_2
                .interrupt_pin
                .clear_interrupt_pending_bit();
            i2c_devices
                .tof_2
                .vl6180x
                .clear_all_interrupts()
                .expect("clrall");
        });
    }

    #[idle()]
    fn idle(_ctx: idle::Context) -> ! {
        loop {}
    }
}
