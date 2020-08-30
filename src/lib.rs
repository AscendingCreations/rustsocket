#![crate_type = "staticlib"]

use libc::*;
use mio::Poll;
use std::boxed::Box;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::ptr;

mod client;
mod server;
use client::ClientState;
use server::{rust_poll_events, Server};

#[no_mangle]
pub extern "C" fn init_socket(
  cpoll: *mut Poll,
  ipaddress: *const c_char,
  client_max: u64,
  recv: extern "C" fn(u64, *const c_char, u64) -> c_int,
  accept: extern "C" fn(u64, *const c_char) -> c_int,
  disconnect: extern "C" fn(u64) -> c_int,
) -> *mut Server {
  if cpoll.is_null() {
    return ptr::null_mut();
  }

  let c_str = unsafe { CStr::from_ptr(ipaddress) };
  let f_str = match c_str.to_str() {
    Ok(n) => n,
    Err(_) => return ptr::null_mut(),
  };

  let server = match Server::new(cpoll, f_str, client_max as usize, recv, accept, disconnect) {
    Ok(n) => n,
    Err(_) => return ptr::null_mut(),
  };

  Box::into_raw(Box::new(server))
}

#[no_mangle]
pub extern "C" fn unload_socket(csocket: *mut Server) {
  unsafe {
    if csocket.is_null() {
      return;
    }

    Box::from_raw(csocket);
  }
}

#[no_mangle]
pub extern "C" fn init_poll() -> *mut Poll {
  let poll = match Poll::new() {
    Ok(n) => n,
    Err(_) => return ptr::null_mut(),
  };

  Box::into_raw(Box::new(poll))
}

#[no_mangle]
pub extern "C" fn unload_poll(cpoll: *mut Poll) {
  unsafe {
    if cpoll.is_null() {
      return;
    }

    Box::from_raw(cpoll);
  }
}

#[no_mangle]
pub extern "C" fn poll_events(cpoll: *mut Poll, cserver: *mut Server) -> c_int {
  if cserver.is_null() || cpoll.is_null() {
    return -1;
  }

  match rust_poll_events(cpoll, cserver) {
    Ok(()) => 0,
    Err(_) => -1,
  }
}

unsafe fn from_buf_raw<T>(ptr: *const T, size: usize) -> Vec<T> {
  let mut dst = Vec::with_capacity(size);
  dst.set_len(size);
  ptr::copy(ptr, dst.as_mut_ptr(), size);
  dst
}

#[no_mangle]
pub extern "C" fn socket_send(
  cserver: *mut Server,
  index: u64,
  data: *const c_char,
  size: u64,
) -> c_int {
  unsafe {
    if cserver.is_null() || data.is_null() {
      return -1;
    }

    let bytes = from_buf_raw(data as *const u8, size as usize);

    match handle_send(&mut *cserver, index, bytes) {
      Ok(_) => return 0,
      Err(_) => return -1,
    }
  }
}

fn handle_send(cserver: &mut Server, index: u64, data: Vec<u8>) -> Result<(), failure::Error> {
  let client = &mut cserver.get_mut(mio::Token(index as usize)).unwrap();
  client.send(data);

  Ok(())
}

#[no_mangle]
pub extern "C" fn socket_set_interest(cserver: *mut Server, index: u64, read: bool) -> c_int {
  unsafe {
    if cserver.is_null() {
      return -1;
    }

    let server = &mut *cserver;

    match server.get_mut(mio::Token(index as usize)) {
      Some(client) => {
        client.has_read = read;
        0
      }
      None => return -1,
    }
  }
}

#[no_mangle]
pub extern "C" fn socket_close(cserver: *mut Server, index: u64) -> c_int {
  unsafe {
    if cserver.is_null() {
      return -1;
    }

    let server = &mut *cserver;

    match server.get_mut(mio::Token(index as usize)) {
      Some(a) => {
        match a.close_socket() {
          Ok(_) => {}
          Err(_) => return -1,
        };

        match a.state {
          ClientState::Closed => {
            server.remove(mio::Token(index as usize));
          }
          _ => {}
        }
      }
      None => {}
    }

    return 0;
  }
}
