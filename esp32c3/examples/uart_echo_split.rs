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
    use corncobs::ZERO;
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
    use rtt_target::{
        //rprint, 
        rprintln, rtt_init_print};
    use shared::{
        serialize_crc_cobs, 
        deserialize_crc_cobs, 
   //     UtcDateTime, 
     //   Id, 
        Command, Message, Response, IN_SIZE, OUT_SIZE};
   
   // const CMD_ARRAY_SIZE: usize = 8;

    
    #[shared]
    struct Shared {
    }

    #[local]
    struct Local {
        in_buf: [u8; IN_SIZE],
        in_buf_index: usize,
    //    rtc: Rtc<'static>,
    //    rtc_offset: u64,
      //  epoch: UtcDateTime,
        timer0: Timer<Timer0<TIMG0>>,
        tx: UartTx<'static, UART0>,
        rx: UartRx<'static, UART0>,
    }

    #[init]
    fn init(_: init::Context) -> (Shared, Local) {
        rtt_init_print!();
        rprintln!("uart_echo_split");

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
        let uart_pins = TxRxPins::new_tx_rx(
            io.pins.gpio0.into_push_pull_output(),
            io.pins.gpio1.into_floating_input(),
        );

        let mut uart0 = Uart::new_with_config(
            peripherals.UART0,
            config,
            Some(uart_pins),
            &clocks,
            &mut system.peripheral_clock_control,
        );
        
        let in_buf = [0u8;IN_SIZE];
        let in_buf_index = 0usize;

        // This is stupid, but i'm not sure how to fixi 
        uart0.set_rx_fifo_full_threshold(1).unwrap();
        uart0.listen_rx_fifo_full();

        timer0.start(1u64.secs());

        let (tx, rx) = uart0.split();

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
            },
        )
    }

    // notice this is not an async task
    #[idle(local = [ timer0 ])]
    fn idle(cx: idle::Context) -> ! {
        loop {
        //  not async wait
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

    fn run_command(cmd_word: Command, tx: &mut UartTx<'_, UART0>, out_buf: &mut [u8;OUT_SIZE]){
       rprintln!("Running command");
       match cmd_word {
       Command::Get(a, b, c) => {rprintln!("Received command Get {} {} {}",a,b,c);},
       Command::Set(a,b,c) => rprintln!("Received command Set {} {:?} {}", a, b, c)
       }
       

       respond(tx, out_buf, Response::SetOk);
       
    }

    // TODO: Implement requesting resend of packet
    fn respond(tx: &mut UartTx<'_, UART0>, out_buf: &mut [u8;OUT_SIZE], message: Response){
        let mut response_failure = false;
        
        match serialize_crc_cobs(&message, out_buf){
            Ok(buf) =>  response_failure = tx.write_bytes(buf).unwrap_or(0usize) == 0,
            Err(_) => rprintln!("Response failed: serialization failed")
        }
        if response_failure{
            rprintln!{"Response failed: Writing to tx failed"}
        }
        *out_buf = [0u8;OUT_SIZE];
    }



    // TODO: Implement sending errmsg to host

    #[task(binds = UART0, priority=2, local = [ rx, tx, in_buf_index, in_buf], shared = [])]
    fn uart0(cx: uart0::Context) {
        let rx = cx.local.rx;
        let tx = cx.local.tx;

        let in_buf = cx.local.in_buf;
        let in_buf_index = cx.local.in_buf_index;
        
        let _cmd_word: Command = Command::Set(0,Message::A,0);
        let mut out_buf = [0u8;OUT_SIZE];

        while let nb::Result::Ok(c) = rx.read() {  
            // Fill buffer with received data
              
            // Reset in_buf array if completely filled
            if *in_buf_index >= IN_SIZE {
                reset_indexed_buf(in_buf, in_buf_index);
                respond(tx, &mut out_buf, Response::NotOk);
            }  

            in_buf[*in_buf_index] = c;
            *in_buf_index += 1;  
            if c == ZERO && *in_buf_index != 0usize {
                // Re-request packet on error. Host handles max retries
                rprintln!("COBS packet recieved");  
                
                match deserialize_crc_cobs(in_buf){
                    Ok(result) => run_command(result, tx, &mut out_buf),
                    Err(_) => respond(tx, &mut out_buf, Response::ParseError)
                }
                reset_indexed_buf(in_buf, in_buf_index);
            }
        rx.reset_rx_fifo_full_interrupt(); 
        }
        
        
    }

}
