//! uart_echo_split
//!
//! Run on target: `cd esp32c3`
//!
//! cargo embed --example uart_echo_split --release
//!
//! Run on host: `cd esp32c3`
//!
//! minicom -b 115200 -D /dev/ttyACM1
//!
//! or
//!
//! moserial -p moserial_acm1.cfg
//!
//! Echoes incoming data
//!
//! This assumes we have usb<->serial adepter appearing as /dev/ACM1
//! - Target TX = GPIO0, connect to RX on adapter
//! - Target RX = GPIO1, connect to TX on adapter
//!
#![no_std]
#![no_main]
#![feature(type_alias_impl_trait)]

use panic_rtt_target as _;

// bring in panic handler
use panic_rtt_target as _;


const CMD_ARRAY_SIZE: usize = 64;

#[rtic::app(device = esp32c3, dispatchers = [FROM_CPU_INTR0, FROM_CPU_INTR1])]
mod app {
    use esp32c3_hal::{
        Rtc,
        clock::ClockControl,
        peripherals::{Peripherals, TIMG0, UART0},
        prelude::*,
        timer::{Timer, Timer0, TimerGroup},
        uart::{
            config::{Config, DataBits, Parity, StopBits},
            TxRxPins, UartRx, UartTx,
        },
        Uart, IO,
    };
    use rtic_sync::{channel::*, make_channel};
    use rtt_target::{rprint, rprintln, rtt_init_print};
    use shared::{serialize_crc_cobs, deserialize_crc_cobs, Command, Message, Response};
    use core::mem::size_of; 
    use corncobs::max_encoded_len; 
    const IN_SIZE: usize = max_encoded_len(size_of::<Command>() + size_of::<u32>()); 
    const OUT_SIZE: usize = max_encoded_len(size_of::<Response>() + size_of::<u32>());
    use serde::{Serialize, Deserialize, de::DeserializeOwned};

    use crate::CMD_ARRAY_SIZE;

    const CAPACITY: usize = 100;

    #[shared]
    struct Shared {
        in_buf: [u8; CMD_ARRAY_SIZE],
        rtc: Rtc<'static>
    }

    #[local]
    struct Local {
        in_buf_index: usize,
        timer0: Timer<Timer0<TIMG0>>,
        tx: UartTx<'static, UART0>,
        rx: UartRx<'static, UART0>,
        sender: Sender<'static, u8, CAPACITY>,
    }

    #[init]
    fn init(_: init::Context) -> (Shared, Local) {
        rtt_init_print!();
        rprintln!("uart_echo_split");
        let (sender, receiver) = make_channel!(u8, CAPACITY);

        let peripherals = Peripherals::take();
        let mut system = peripherals.SYSTEM.split();
        let clocks = ClockControl::max(system.clock_control).freeze();

        let timer_group0 = TimerGroup::new(
            peripherals.TIMG0,
            &clocks,
            &mut system.peripheral_clock_control,
        );
        let mut timer0 = timer_group0.timer0;

        let rtc: Rtc<'_> = Rtc::new(peripherals.RTC_CNTL);

        let config = Config {
            baudrate: 115200,
            data_bits: DataBits::DataBits8,
            parity: Parity::ParityNone,
            stop_bits: StopBits::STOP1,
        };

        let io = IO::new(peripherals.GPIO, peripherals.IO_MUX);
        let pins = TxRxPins::new_tx_rx(
            io.pins.gpio0.into_push_pull_output(),
            io.pins.gpio1.into_floating_input(),
        );

        let mut uart0 = Uart::new_with_config(
            peripherals.UART0,
            config,
            Some(pins),
            &clocks,
            &mut system.peripheral_clock_control,
        );
        
        let in_buf = [0u8;CMD_ARRAY_SIZE];
        let in_buf_index = 0usize;

        // This is stupid!
        // TODO, use at commands with break character
        uart0.set_rx_fifo_full_threshold(1).unwrap();
        uart0.listen_rx_fifo_full();

        timer0.start(1u64.secs());

        let (tx, rx) = uart0.split();

        lowprio::spawn(receiver).unwrap();

        (
            Shared {
                in_buf,
                rtc
            },
            Local {
                in_buf_index,
                timer0,
                tx,
                rx,
                sender,
            },
        )
    }

    // notice this is not an async task
    #[idle(local = [ timer0 ])]
    fn idle(cx: idle::Context) -> ! {
        loop {
            rprintln!("idle, do some background work if any ...");
            // not async wait
            nb::block!(cx.local.timer0.wait()).unwrap();
        }
    }

    fn reset_cmd_array(in_buf: &mut [u8;CMD_ARRAY_SIZE], in_buf_index: &mut usize){
        *in_buf = [0u8;CMD_ARRAY_SIZE];
        *in_buf_index = 0;
    }



    fn run_command(in_buf: &[u8;CMD_ARRAY_SIZE]) -> Result<&str,&str>{
        /*match in_buf{
            [115, 101, 116, 116, 105, 109, 101] => (),

        };
        serialize_crc_cobs(in_buf);
        */for character in in_buf{
            rprint!("{}", *character as char);
            
        }
        Ok("Success")
    }


    #[task(binds = UART0, priority=2, local = [ rx, sender, in_buf_index], shared = [in_buf])]
    fn uart0(cx: uart0::Context) {
        let rx = cx.local.rx;
        let sender = cx.local.sender;
        let mut in_buf = cx.shared.in_buf;
        let in_buf_index = cx.local.in_buf_index;
        let mut cmd_word_array = [0u8;CMD_ARRAY_SIZE];
        let cmd_word: [u8;8] = [0u8;8];

        rprintln!("Interrupt Received: ");

        while let nb::Result::Ok(c) = rx.read() {
            rprint!("{}", c as char);
            rprintln!();    
            
                    
            // Fill buffer with inputted data
            in_buf.lock(|in_buf|{
                if c == 0x00 {
                    rprintln!("COBS packet recieved");                    
                    cmd_word = deserialize_crc_cobs(in_buf).unwrap();



                    for character in cmd_word{
                        rprint!("{}", character as char);
                    }
                    reset_cmd_array(in_buf, in_buf_index);  
                }

                // Reset in_buf array if completely filled
                
                if *in_buf_index >= CMD_ARRAY_SIZE {
                    reset_cmd_array(in_buf, in_buf_index)
                }
                    
                in_buf[*in_buf_index] = c;
                                        
                //Debugging array contents print
                for character in in_buf{
                    rprint!("{}", *character as char);
                }
                

                *in_buf_index += 1;

            });


            if cmd_word_array[0] != 0{
                let cmd_wrd = 
                rprint!("{}", cmd_wrd);
                cmd_word_array = [0u8;CMD_ARRAY_SIZE];
            }

            match sender.try_send(c) {
                Err(_) => {
                    rprintln!("send buffer full");
                }
                _ => {}
            }
            
        }
        
        rprintln!("");
        rx.reset_rx_fifo_full_interrupt()
    }

    #[task(priority = 1, local = [ tx ])]
    async fn lowprio(cx: lowprio::Context, mut receiver: Receiver<'static, u8, CAPACITY>) {
        rprintln!("LowPrio started");
        let tx = cx.local.tx;

        while let Ok(c) = receiver.recv().await {
            rprintln!("Receiver got: {}", c);
            if c == 8{
                tx.write(8).unwrap();
                tx.write(32).unwrap();
            }
            tx.write(c).unwrap();
            if c == 13{
                tx.write(10).unwrap();
               // tx.write(62).unwrap();
                //tx.write(32).unwrap();
            }

        }
    }
}
