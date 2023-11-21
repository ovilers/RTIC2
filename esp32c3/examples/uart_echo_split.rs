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

    use crate::CMD_ARRAY_SIZE;

    const CAPACITY: usize = 100;

    #[shared]
    struct Shared {
        command: [u8; CMD_ARRAY_SIZE],
    }

    #[local]
    struct Local {
        cmdindex: usize,
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
        
        let command = [0u8;CMD_ARRAY_SIZE];
        let cmdindex = 0usize;

        // This is stupid!
        // TODO, use at commands with break character
        uart0.set_rx_fifo_full_threshold(1).unwrap();
        uart0.listen_rx_fifo_full();

        timer0.start(1u64.secs());

        let (tx, rx) = uart0.split();

        lowprio::spawn(receiver).unwrap();

        (
            Shared {
                command,
            },
            Local {
                cmdindex,
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

    fn reset_cmd_array(command: &mut [u8;CMD_ARRAY_SIZE], cmdindex: &mut usize){
        *command = [0u8;CMD_ARRAY_SIZE];
        *cmdindex = 0;
    }



    fn run_command(command: &[u8;CMD_ARRAY_SIZE]){
        rprintln!("Running command: ");
        for character in command{
            rprint!("{}", *character as char);
        }
    }


    #[task(binds = UART0, priority=2, local = [ rx, sender, cmdindex], shared = [command])]
    fn uart0(cx: uart0::Context) {
        let rx = cx.local.rx;
        let sender = cx.local.sender;
        let mut command = cx.shared.command;
        let cmdindex = cx.local.cmdindex;

        let mut cmd_word = [0u8;CMD_ARRAY_SIZE];

        rprintln!("Interrupt Received: ");

        while let nb::Result::Ok(c) = rx.read() {
            rprint!("{}", c as char);
            rprintln!();    
            
                    
            
            command.lock(|command|{
                if c == 13 {
                    rprintln!("Enter pressed");                    
                    // Extract command to minimize blocking
                    cmd_word = *command;
                    for character in *command{
                        rprint!("{}", character as char);
                    }
                    reset_cmd_array(command, cmdindex);  
                }

                // Reset command array if completely filled
                else{
                    if c == 8 && *cmdindex > 0 {
                        rprintln!("Removing character");
                        command[*cmdindex-1] = 0;
                        *cmdindex -= 1;
                    }

                    if *cmdindex >= CMD_ARRAY_SIZE {
                        reset_cmd_array(command, cmdindex)
                    }
                    
                    if c != 8{
                        command[*cmdindex] = c;
                    }
                                        
                    //Debugging array contents print
                    for character in command{
                        rprint!("{}", *character as char);
                    }
                }

                //Increment command index if not backspace or enter
                if c != 8 && c != 13 {*cmdindex += 1};

            });

        
            if cmd_word[0] != 0{
                run_command(&cmd_word);
                cmd_word = [0u8;CMD_ARRAY_SIZE];
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
