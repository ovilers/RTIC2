//! host side application
//!
//! Run on target `cd esp32c3`
//!
//! cargo embed --example cmd_crc_cobs_lib --release
//!
//! Run on host `cd host`
//!
//! cargo run
//!

// Rust dependencies
use std::{io::{Read, Error},io, mem::size_of};

// Libraries
use corncobs::{max_encoded_len, ZERO};
use serial2::SerialPort;

// Application dependencies
use host::open;
use shared::{deserialize_crc_cobs, serialize_crc_cobs, Command, Message, Response}; // local library

const IN_SIZE: usize = max_encoded_len(size_of::<Response>() + size_of::<u32>());
const OUT_SIZE: usize = max_encoded_len(size_of::<Command>() + size_of::<u32>());
const MAX_RETRIES: usize = 3;

type InBuf = [u8; IN_SIZE];
type OutBuf = [u8; OUT_SIZE];

fn main() -> Result<(), std::io::Error> {
    let mut port = open()?;

    let mut out_buf = [0u8; OUT_SIZE];
    let mut in_buf = [0u8; IN_SIZE];

    loop{
        let mut input_raw = String::new();

        print_commands();

        match io::stdin().read_line(&mut input_raw) {
            Err(error) => println!("Error with input: {error}"),
            _ => {}
            }
        let input = input_raw.as_str();
        let cmd: Command = match input{
            "1" => Command::Set(0x12, Message::B(12), 0b001),
            "2" => Command::Get(0x12, 12, 0b001),
            "3" => Command::Set(0x01, Message::B(1), 0b000),
            "4" => Command::Get(0x01, 15, 0b000 ),
            "h" => {print_commands(); continue;},
            "q" => return Ok(()),
            _ => {println!("Invalid input"); continue;}
        };

        println!("request {:?}", cmd);
        let response = request(&cmd, &mut port, &mut out_buf, &mut in_buf);
        match response{
            Ok(m) => println!("Success {:?}", m),
            Err(m) => println!("Failure: {:?}", m),
        }
    }  
   // Ok(())
}

fn print_commands(){
    println!("Greetings! Please input your command:");
    println!(" 1  Set RTC of esp");
    println!(" 2  Get RTC of esp");
    println!(" 3  Set colour of LED to setting 1");
    println!(" 4  Get colour of LED");
    println!(" h  print this message");
    println!(" q  Exit program");

}

fn request(
    cmd: &Command,
    port: &mut SerialPort,
    out_buf: &mut OutBuf,
    in_buf: &mut InBuf,
) -> Result<Response, std::io::Error> {
    let mut retries: usize = 0;
   
    while retries < MAX_RETRIES{    
        println!("out_buf {}", out_buf.len());
   
        let to_write = serialize_crc_cobs(cmd, out_buf);
        match to_write{
            Ok(val) => port.write_all(val).unwrap(),
            Err(m) => println!("Serialization error: {:?}", m)
        }

        let mut index: usize = 0;
        loop {
            let slice = &mut in_buf[index..index + 1];
            if index < IN_SIZE {
                index += 1;
            }
            port.read_exact(slice)?;
            if slice[0] == ZERO {
                println!("-- cobs package received --");
                break;
            }
   
        }
        println!("cobs index {}", index);
        if index <= 1{
            return Err(Error::new (std::io::ErrorKind::InvalidData, "Packet empty"));
        }
        match deserialize_crc_cobs(in_buf){
            Ok(response) => {
                match response{
                    Response::SetOk => return Ok(response),
                    _ => {println!("Client failed to parse packet!");},
                }},
            Err(ssmarshal::Error::ApplicationError("Crc Mismatch")) => println!("CRC mismatch!"),
            _ => println!("Could not parse packet!"),
        };
        println!("Retrying...");
        retries += 1;
    };
    println!("Failed to send command");
    Err(Error::new(std::io::ErrorKind::BrokenPipe, "Failed to send command"))
}
