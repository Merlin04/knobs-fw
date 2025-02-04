#![no_std]
#![no_main]
#![feature(abi_avr_interrupt)]
#![feature(generic_arg_infer)]

mod millis;
mod usb;

use core::ops::DerefMut;

use arduino_hal::Usart;
use atmega_hal::usart::BaudrateExt;
use avr_device::asm::sleep;
use avr_device::interrupt;

use millis::{millis, millis_init};
use usb_device::{descriptor::lang_id::LangID, prelude::StringDescriptors};

use panic_halt as _;
use usbd_midi::{CableNumber, UsbMidiEventPacket};

use analog_multiplexer::{DummyPin, Multiplexer};

use midi_convert::{midi_types::{Channel, Control, MidiMessage, Value7}, render_slice::MidiRenderSlice};

const CH_MAP: [u8; 16] = [
    15,
    14,
    13,
    12,
    11,
    10,
    9,
    8,
    0,
    1,
    2,
    3,
    4,
    5,
    6,
    7
];

#[derive(Default)]
struct Knob {
    val: u8,
    prev: u8,
    ms: u32
}

#[arduino_hal::entry]
fn main() -> ! {
    let dp = arduino_hal::Peripherals::take().unwrap();

    millis_init(dp.TC0);

    let midi = usb::init(dp.PLL, dp.USB_DEVICE, StringDescriptors::new(LangID::EN)
        .manufacturer("m04")
        .product("knobs"));

    let pins = atmega_hal::pins!(dp);

    let mut usart = Usart::new(
        dp.USART1,
        pins.pd2,
        pins.pd3.into_output(),
        BaudrateExt::into_baudrate(31250)
    );
    
    let s0 = pins.pf5.into_output();
    let s1 = pins.pf6.into_output();
    let s2 = pins.pf7.into_output();
    let s3 = pins.pb1.into_output();
    let m_pins = (s0, s1, s2, s3, DummyPin {});
    let mut multiplexer = Multiplexer::new(m_pins);
    multiplexer.enable();

    let mut adc = arduino_hal::Adc::new(dp.ADC, Default::default());
    let analog_pin = pins.pf4.into_analog_input(&mut adc);

    let mut vals: [Knob; 16] = Default::default();
    
    loop {
        for ch in 0..multiplexer.num_channels {
            multiplexer.set_channel(ch);

            let val = (analog_pin.analog_read(&mut adc) >> 3) as u8;
            // let val: u8 = (((analog_pin.analog_read(&mut adc) as f32) / (u16::MAX as f32)) * (u8::MAX as f32)) as u8;

            let ms = millis();

            let k = &mut vals[ch as usize];

            if k.val != val {
                let should_ignore = val == k.prev && ms > k.ms + 50;

                if !should_ignore {
                    let message = MidiMessage::ControlChange(
                        Channel::new(0),
                        Control::new(101 + CH_MAP[ch as usize]),
                        Value7::new(val)
                    );
                    let mut bytes = [0; 3];
                    message.render_slice(&mut bytes);
                    
                    for b in bytes {
                        usart.write_byte(b);
                    }
    
                    let packet = UsbMidiEventPacket::try_from_payload_bytes(CableNumber::Cable0, &bytes).unwrap();
                    
                    interrupt::free(|cs| {
                        let mut binding = midi.borrow(cs).borrow_mut();
                        let midi = binding.deref_mut();
                        let _ = midi.send_packet(packet);
                    });
                }

                k.prev = k.val;
                k.val = val;
                k.ms = ms;
            }
        }
        sleep();
    }
}