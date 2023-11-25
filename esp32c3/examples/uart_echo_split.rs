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
// use corncobs::max_encoded_len;
// use core::mem::size_of;



#[rtic::app(device = esp32c3, dispatchers = [FROM_CPU_INTR0, FROM_CPU_INTR1])]
mod app {
    use esp32c3_hal::{
 //       Rtc,
        clock::ClockControl,
        peripherals::{Peripherals, TIMG0, UART0},
        prelude::*,
        timer::{Timer, Timer0, TimerGroup},
        uart::{
            config::{Config, DataBits, Parity, StopBits},
            TxRxPins, UartRx, UartTx,
        },
        Uart, IO, 
        // riscv::register::medeleg::Medeleg,
    };
    use rtic_sync::{channel::*, make_channel};
    use rtt_target::{
        //rprint, 
        rprintln, rtt_init_print};
    use shared::{
     //   serialize_crc_cobs, 
        deserialize_crc_cobs, 
   //     UtcDateTime, 
     //   Id, 
        Command, Message, Response, DeserError};
    use core::mem::size_of; 
    use corncobs::{max_encoded_len, 
    //    ZERO
    }; 
   // const IN_SIZE: usize = max_encoded_len(size_of::<Command>() + size_of::<u32>()); 
    //const OUT_SIZE: usize = max_encoded_len(size_of::<Response>() + size_of::<u32>());
    const BUF_ARRAY_SIZE: usize = max_encoded_len(size_of::<Response>() + size_of::<u32>());
   // const CMD_ARRAY_SIZE: usize = 8;

    

    const CAPACITY: usize = 100;

    #[shared]
    struct Shared {
    }

    #[local]
    struct Local {
        in_buf: [u8; BUF_ARRAY_SIZE],
        in_buf_index: usize,
    //    rtc: Rtc<'static>,
    //    rtc_offset: u64,
      //  epoch: UtcDateTime,
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

  //      let rtc: Rtc<'_> = Rtc::new(peripherals.RTC_CNTL);
   //     let rtc_offset = 0;

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
        
        let in_buf = [0u8;BUF_ARRAY_SIZE];
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
            },
            Local {
                in_buf_index,
     //           rtc,
                in_buf,
       //         epoch,
     //           rtc_offset,
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

    

    fn reset_indexed_buf<const N: usize>(buf: &mut [u8;N], buf_index: &mut usize){
        *buf = [0u8;N];
        *buf_index = 0;
    }
/*
    fn set_rtc(rtc: Rtc<'static>, epoch: u8, id:Id, message:Message) -> Response{
        let mut data = 0u32;
        match message{
          //  Message::B(content) => data = content,
            _ => return Response::ParseError
        }

       /* match id{
            1 => epoch.year = data,
            2 => epoch.month = data,
            3 => epoch.day = data,
            4 => epoch.hour = data,
            5 => epoch.minute = data,
            6 => epoch.second = data,
            _ => return Response::ParseError
        }
        */
    }
*/

    fn run_command(cmd_word: Command){
       
       match cmd_word {
       _ => {}
           
       }
     //  Response::SetOk;
       
    }

    // TODO: Implement requesting resend of packet
    fn request_resend(){

    }



    // TODO: Implement sending errmsg to host
    fn send_errmsg(response: Response, sender: &mut Sender<'_, u8, 100> ){
        match response{
            Response::ParseError => {sender.try_send('E' as u8).unwrap();}
            ,
            _ => {}
        }
    }

    #[task(binds = UART0, priority=2, local = [ rx, sender, in_buf_index, in_buf], shared = [])]
    fn uart0(cx: uart0::Context) {
        let rx = cx.local.rx;
        let sender = cx.local.sender;
        let in_buf = cx.local.in_buf;
        let in_buf_index = cx.local.in_buf_index;
        
        let _cmd_word: Command = Command::Set(0,Message::A,0);
        let _out_buf = [0u8;BUF_ARRAY_SIZE];

        rprintln!("Interrupt Received: ");

        while let nb::Result::Ok(c) = rx.read() {  
            // Fill buffer with received data
        
            if c == 13 && *in_buf_index != 0usize {
                // TODO: Re-request packet on CrcError, send error message on ParseError
                rprintln!("COBS packet recieved");  
                
                match deserialize_crc_cobs(in_buf){
                    Ok(result) => run_command(result),
                    Err(error) => {
                        match error{
                            DeserError::CrcError => {request_resend();},
                            _ => {send_errmsg(Response::ParseError, sender);}
                    };
                    }
                }
                reset_indexed_buf(in_buf, in_buf_index);
            }
            else{
                // Reset in_buf array if completely filled
                if *in_buf_index >= BUF_ARRAY_SIZE {
                    reset_indexed_buf(in_buf, in_buf_index);
                    send_errmsg(Response::ParseError, sender);
                    }  

                in_buf[*in_buf_index] = c;
                *in_buf_index += 1;     
                }
        rprintln!("Received char {}, {}",c , c as char);
        rprintln!("");
        rx.reset_rx_fifo_full_interrupt(); 
        }
        
        
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
