#![cfg_attr(not(test), no_std)]

pub mod date_time;
pub mod shift_register;

use serde_derive::{Deserialize, Serialize};

// we could use new-type pattern here but let's keep it simple
pub type Id = u32;
pub type DevId = u32;
pub type Parameter = u32;

#[derive(Debug, Serialize, Deserialize)]
#[repr(C)]
pub enum Command {
    Set(Id, Message, DevId),
    Get(Id, Parameter, DevId),
}

#[derive(Debug, Serialize, Deserialize)]
#[repr(C)]
pub enum Message {
    A,
    B(u32),
    C(f32), // we might consider "f16" but not sure it plays well with `ssmarshal`
}

#[derive(Debug, Serialize, Deserialize)]
#[repr(C)]
pub enum Response {
    Data(Id, Parameter, u32, DevId),
    SetOk,
    ParseError,
}



pub const CKSUM: crc::Crc<u32> = crc::Crc::<u32>::new(&crc::CRC_32_CKSUM);

/// Serialize T into cobs encoded out_buf with crc
/// panics on all errors
/// TODO: reasonable error handling
pub fn serialize_crc_cobs<'a, T: serde::Serialize, const N: usize>(
    t: &T,
    out_buf: &'a mut [u8; N],
) -> Result<&'a [u8], ssmarshal::Error> {

    let n_ser = match ssmarshal::serialize(out_buf, t){
        Ok(value) => value,
        Err(m) => return Err(m)
    };

    let crc = CKSUM.checksum(&out_buf[0..n_ser]);
    let n_crc = match ssmarshal::serialize(&mut out_buf[n_ser..], &crc){
        Ok(value) => value,
        Err(m) => return Err(m)
    };
    let buf_copy = (*out_buf).clone();
    let n = corncobs::encode_buf(&buf_copy[0..n_ser + n_crc], out_buf);
    Ok(&out_buf[0..n])
}

pub enum DeserError{
    CrcError,
    ParseError
}

/// deserialize T from cobs in_buf with crc check
/// On error returns CrcError on unmatching CRC, otherwise ParseError
/// 
pub fn deserialize_crc_cobs<T>(in_buf: &mut [u8]) -> Result<T, DeserError>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let n = match corncobs::decode_in_place(in_buf){
        Ok(value) => value,
        Err(m) => return Err(DeserError::ParseError)
    };
    let (t, resp_used) = match ssmarshal::deserialize::<T>(&in_buf[0..n]){
        Ok(value) => value,
        Err(m) => return Err(DeserError::ParseError)
    };
    let crc_buf = &in_buf[resp_used..];
    let (crc, _crc_used) = match ssmarshal::deserialize::<u32>(crc_buf){
        Ok(value) => value,
        Err(m) => return Err(DeserError::ParseError)
    };
    let pkg_crc = CKSUM.checksum(&in_buf[0..resp_used]);
    if crc != pkg_crc{
        return Err(DeserError::CrcError)
    };
    Ok(t)
}
