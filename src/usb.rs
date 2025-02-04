use core::{cell::RefCell, ops::DerefMut};

use arduino_hal::{pac::PLL, usb::AvrGenericUsbBus};

use atmega_hal::pac::USB_DEVICE;
use avr_device::interrupt::Mutex;
use avr_device::interrupt;

use static_cell::StaticCell;
use usb_device::{bus::UsbBusAllocator, device::UsbDevice, prelude::StringDescriptors};

use panic_halt as _;
use usbd_midi::{UsbMidiClass, UsbMidiPacketReader};

// https://github.com/supersimple33/avr-hal/blob/main/examples/arduino-leonardo/src/bin/leonardo-usb.rs

type MidiBus = Mutex<RefCell<UsbMidiClass<'static, AvrGenericUsbBus<PLL>>>>;

static mut USB_CTX: Option<UsbContext> = None;

struct UsbContext {
    usb_device: UsbDevice<'static, AvrGenericUsbBus<PLL>>,
    midi: &'static MidiBus
}

pub fn init(pll: PLL, usb: USB_DEVICE, strings: StringDescriptors<'static>) -> &'static MidiBus {
    // Configure PLL interface
    // prescale 16MHz crystal -> 8MHz
    pll.pllcsr.write(|w| w.pindiv().set_bit());
    // 96MHz PLL output; /1.5 for 64MHz timers, /2 for 48MHz USB
    pll.pllfrq
        .write(|w| w.pdiv().mhz96().plltm().factor_15().pllusb().set_bit());

    // Enable PLL
    pll.pllcsr.modify(|_, w| w.plle().set_bit());

    // Check PLL lock
    while pll.pllcsr.read().plock().bit_is_clear() {}


    let usb_bus: &UsbBusAllocator<AvrGenericUsbBus<PLL>> = arduino_hal::default_usb_bus!(usb, pll);

    static MIDI: StaticCell<MidiBus> = StaticCell::new();
    let midi = MIDI.init(Mutex::new(RefCell::new(UsbMidiClass::new(&usb_bus, 1, 1).unwrap())));

    let usb_device: UsbDevice<AvrGenericUsbBus<PLL>> = arduino_hal::default_usb_device!(usb_bus, 0xecbb, 0xec02, strings);

    unsafe {
        USB_CTX = Some(UsbContext {
            usb_device,
            midi
        });
    }

	unsafe { interrupt::enable() };

    midi
}

impl UsbContext {
    fn poll(&mut self) {
        interrupt::free(|cs| {
            let mut binding = self.midi.borrow(cs).borrow_mut();
            let midi = binding.deref_mut();
            if self.usb_device.poll(&mut [midi]) {
                let mut buffer = [0; 64];
    
                if let Ok(size) = midi.read(&mut buffer) {
                    let packet_reader = UsbMidiPacketReader::new(&buffer, size);
                    for packet in packet_reader.into_iter().flatten() {
                        if !packet.is_sysex() {
                            // regular midi message
                            
                        } else {
                            // sysex
    
                        }
                    }
                }
            }    
        });
    }
}

#[interrupt(atmega32u4)]
fn USB_GEN() {
    unsafe { poll_usb() };
}

#[interrupt(atmega32u4)]
fn USB_COM() {
    unsafe { poll_usb() };
}

/// # Safety
///
/// This function assumes that it is being called within an
/// interrupt context.
unsafe fn poll_usb() {
    // Safety: There must be no other overlapping borrows of USB_CTX.
    // - By the safety contract of this function, we are in an interrupt
    //   context.
    // - The main thread is not borrowing USB_CTX. The only access is the
    //   assignment during initialization. It cannot overlap because it is
    //   before the call to `interrupt::enable()`.
    // - No other interrupts are accessing USB_CTX, because no other interrupts
    //   are in the middle of execution. GIE is automatically cleared for the
    //   duration of the interrupt, and is not re-enabled within any ISRs.
    #[expect(static_mut_refs)]
    let ctx = unsafe { USB_CTX.as_mut().unwrap() };
    ctx.poll();
}