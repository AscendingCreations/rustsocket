use libc::*;
use mio::net::TcpStream;
use mio::Interest;
use std::io::{self, Read, Write};
use std::os::raw::c_char;

#[derive(Debug, Copy, Clone)]
pub enum ClientState {
  Open,
  Closing,
  Closed,
}

#[derive(Debug)]
pub struct Client {
  pub stream: TcpStream,
  pub token: mio::Token,
  pub state: ClientState,
  pub sends: Vec<Vec<u8>>,
  pub has_read: bool,
  pub recv: unsafe extern "C" fn(u64, *const c_char, u64) -> c_int,
  pub acpt: unsafe extern "C" fn(u64, *const c_char) -> c_int,
  pub disconnect: unsafe extern "C" fn(u64) -> c_int,
}

impl Client {
  pub fn new(
    stream: TcpStream,
    token: mio::Token,
    recv: unsafe extern "C" fn(u64, *const c_char, u64) -> c_int,
    acpt: unsafe extern "C" fn(u64, *const c_char) -> c_int,
    disconnect: unsafe extern "C" fn(u64) -> c_int,
  ) -> Client {
    Client {
      stream: stream,
      token: token,
      state: ClientState::Open,
      sends: Vec::new(),
      has_read: true,
      recv,
      acpt,
      disconnect,
    }
  }

  pub fn process(
    &mut self,
    poll: &mio::Poll,
    event: &mio::event::Event,
  ) -> Result<(), failure::Error> {
    if event.is_readable() {
      self.has_read = false;
      self.read();
    }

    if event.is_writable() {
      self.write();
    }

    match self.state {
      ClientState::Closing => self.close_socket()?,
      _ => self.reregister(poll)?,
    }

    Ok(())
  }

  pub fn close_socket(&mut self) -> Result<(), failure::Error> {
    match self.state {
      ClientState::Closed => return Ok(()),
      _ => {
        self.stream.shutdown(std::net::Shutdown::Both)?;
        self.state = ClientState::Closed;
        unsafe {
          (self.disconnect)(self.token.0 as u64);
        }
        Ok(())
      }
    }
  }

  pub fn read(&mut self) {
    let mut buffer_list: Vec<u8> = Vec::new();
    loop {
      let mut buf: [u8; 2048] = [0; 2048];
      match self.stream.read(&mut buf) {
        Ok(0) => {
          self.state = ClientState::Closing;
          return;
        }
        Ok(n) => {
          for i in 0..n {
            buffer_list.push(buf[i]);
          }
        }
        Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => break,
        Err(ref err) if err.kind() == io::ErrorKind::Interrupted => continue,
        Err(_) => {
          self.state = ClientState::Closing;
          return;
        }
      }
    }

    if !buffer_list.is_empty() {
      unsafe {
        (self.recv)(
          self.token.0 as u64,
          buffer_list[..].as_ptr() as *const i8,
          buffer_list.len() as u64,
        );
      }
    }
  }

  pub fn write(&mut self) {
    loop {
      let mut buffer = match self.sends.pop() {
        Some(buffer) => buffer,
        None => {
          self.sends.shrink_to_fit();
          return;
        }
      };

      match self.stream.write(&mut buffer) {
        Ok(n) => {
          if n == 0 {
            self.state = ClientState::Closing;
            return;
          }
        }
        Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => break,
        Err(ref err) if err.kind() == io::ErrorKind::Interrupted => continue,
        Err(_) => {
          self.state = ClientState::Closing;
          return;
        }
      }
    }
  }

  pub fn event_set(&mut self) -> Interest {
    if self.has_read {
      Interest::READABLE.add(Interest::WRITABLE)
    } else {
      Interest::WRITABLE
    }
  }

  pub fn register(&mut self, poll: &mio::Poll) -> Result<(), failure::Error> {
    let interest = self.event_set();
    poll.registry().register(
      &mut self.stream,
      self.token,
      interest,
    )?;
    Ok(())
  }

  pub fn reregister(&mut self, poll: &mio::Poll) -> Result<(), failure::Error> {
    let interest = self.event_set();
    poll.registry().reregister(
      &mut self.stream,
      self.token,
      interest,
    )?;
    Ok(())
  }

  pub fn send(&mut self, buf: Vec<u8>) {
    self.sends.insert(0, buf);
  }
}
