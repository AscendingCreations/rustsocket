use mio::net::TcpListener;
use mio::{Events, Poll};
use std::collections::{HashMap, VecDeque};
use libc::*;
use std::time::Duration;
use std::ffi::CString;

use crate::client::{Client, ClientState};

pub const SERVER: mio::Token = mio::Token(0);

#[repr(C)]
#[derive(Debug)]
pub struct Server {
    pub listener: TcpListener,
    pub clients: HashMap<mio::Token, Client>,
    pub tokens: VecDeque<mio::Token>,
    pub recv: unsafe extern "C" fn(u64, *const c_char, u64) -> c_int,
    pub acpt: unsafe extern "C" fn(u64, *const c_char) -> c_int,
    pub disconnect: unsafe extern "C" fn(u64) -> c_int
}

impl Server {
    pub fn new(poll: *mut Poll, addr: &str, max: usize,
        recv: unsafe extern "C"  fn(u64, *const c_char, u64) -> c_int,
        acpt: unsafe extern "C"  fn(u64, *const c_char) -> c_int,
        disconnect: unsafe extern "C"  fn(u64) -> c_int
    ) -> Result<Server, failure::Error> {
        /* Create a bag of unique tokens. */
        let mut tokens = VecDeque::new();

        for i in 1..=max {
            tokens.push_back(mio::Token(i));
        }

        /* Set up the TCP listener. */
        let addr = addr.parse()?;
        let mut listener = TcpListener::bind(addr)?;

        unsafe {
          poll.as_ref().unwrap().registry()
              .register(&mut listener, SERVER, mio::Interest::READABLE)?;
        }

        Ok(Server {
            listener: listener,
            clients: HashMap::new(),
            tokens: tokens,
            recv,
            acpt,
            disconnect
        })
    }

    pub fn accept(&mut self, poll: &Poll) -> Result<(), failure::Error> {
        /* Wait for a new connection to accept and try to grab a token from the bag. */
        let (stream, addr) = self.listener.accept()?;
        let token = self.tokens.pop_front();

        if let Some(token) = token {
            /* We got a unique token, now let's register the new connection. */
            println!("Connection received from {}", addr);
            let mut client = Client::new(stream, token, self.recv, self.acpt, self.disconnect);
            client.register(poll)?;
            self.clients.insert(token, client);
            let cstr = CString::new(addr.to_string()).expect("CString::new failed");
            let raw = cstr.into_raw();

            unsafe {
                if (self.acpt)(token.0 as u64, raw) < 0 {
                    return Err(failure::err_msg("Failed to accept user."));
                }
            }
        } else {
            /* We ran out of tokens, disconnect the new connection. */
            println!("Reached maximum number of clients, disconnected {}", addr);
            drop(stream);
        }

        Ok(())
    }

    pub fn get_mut(&mut self, token: mio::Token) -> Option<&mut Client> {
        /* Look up the connection for the given token. */
        self.clients.get_mut(&token)
    }

    pub fn get(&mut self, token: mio::Token) -> Option<&Client> {
      /* Look up the connection for the given token. */
      self.clients.get(&token)
    }

    pub fn remove(&mut self, token: mio::Token) {
        /* If the token is valid, let's remove the connection and add the token back to the bag. */
        if self.clients.contains_key(&token) {
            self.clients.remove(&token);
            self.tokens.push_front(token);
        }
    }
}

pub fn rust_poll_events(cpoll: *mut Poll, cserver: *mut Server) -> Result<(), failure::Error> {

    unsafe {
    let poll = &mut *cpoll;
    let server = &mut *cserver;
    //let mut server = &mut *cserver;

    let mut events = Events::with_capacity(256);

    poll.poll(&mut events, Some(Duration::from_millis(500)))?;

      for event in events.iter() {
        match event.token() {
            SERVER => {server.accept(&poll)?;
              poll.registry()
              .reregister(&mut server.listener, SERVER, mio::Interest::READABLE)?;
            },
            token => {
                match server.get_mut(token) {
                  Some(a) => {
                    a.process(&poll, &event)?;

                    match a.state {
                      ClientState::Closed => {
                        server.remove(token);
                      }
                      _ => {}
                    }
                  },
                  None => {}
                }
            }
        }
      }
  }

    Ok(())
}