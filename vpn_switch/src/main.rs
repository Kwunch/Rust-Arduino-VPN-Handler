#![no_std]
#![no_main]

use panic_halt as _;

#[arduino_hal::entry]
fn main() -> ! {
    let dp = arduino_hal::Peripherals::take().unwrap();
    let pins = arduino_hal::pins!(dp);
    let mut serial = arduino_hal::default_serial!(dp, pins, 57600);

    let mut red_led = pins.d4.into_output();
    let mut blue_led = pins.d11.into_output();
    
    let switch = pins.d7.into_pull_up_input();
    
    loop {
        if switch.is_low() {
            red_led.set_high();
            blue_led.set_low();
            ufmt::uwriteln!(&mut serial, "Turn Off").unwrap();
        } else {
            red_led.set_low();
            blue_led.set_high();
            ufmt::uwriteln!(&mut serial, "Turn On").unwrap();
        }
    }
}
