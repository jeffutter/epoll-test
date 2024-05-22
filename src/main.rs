use anyhow::{anyhow, Result};
use byteorder::{ReadBytesExt, WriteBytesExt};
use env_logger::Env;
use log::{debug, error, info};
use rand::Rng;
use std::{
    collections::HashMap,
    net::{TcpListener, TcpStream},
    os::fd::{AsRawFd, RawFd},
    thread,
    time::{Duration, SystemTime},
};

const READ_FLAGS: i32 = libc::EPOLLONESHOT | libc::EPOLLIN;

#[allow(unused_macros)]
macro_rules! syscall {
    ($fn: ident ( $($arg: expr),* $(,)* ) ) => {{
        let res = unsafe { libc::$fn($($arg, )*) };
        if res == -1 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(res)
        }
    }};
}

fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let server_handle = thread::spawn(|| -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:8000")?;
        listener.set_nonblocking(true)?;
        let listener_fd = listener.as_raw_fd();

        let fd = syscall!(epoll_create1(0))?;
        if let Ok(flags) = syscall!(fcntl(fd, libc::F_GETFD)) {
            let _ = syscall!(fcntl(fd, libc::F_SETFD, flags | libc::FD_CLOEXEC));
        }

        let mut key = 100;
        add_interest(fd, listener_fd, listener_read_event(key))?;

        let mut events: Vec<libc::epoll_event> = Vec::with_capacity(1024);
        let mut contexts: HashMap<u64, TcpStream> = HashMap::new();

        loop {
            events.clear();
            let res = match syscall!(epoll_wait(
                fd,
                events.as_mut_ptr(),
                1024,
                1000 as libc::c_int,
            )) {
                Ok(v) => v,
                Err(e) => panic!("error during epoll wait: {}", e),
            };

            unsafe { events.set_len(res as usize) };

            for ev in &events {
                match ev.u64 {
                    100 => {
                        debug!("Epoll event received: {:?}", ev);
                        match listener.accept() {
                            Ok((stream, addr)) => {
                                debug!("new client: {}", addr);

                                stream.set_nonblocking(true)?;
                                key += 1;
                                add_interest(fd, stream.as_raw_fd(), listener_read_event(key))?;
                                contexts.insert(key, stream);
                            }
                            Err(e) => error!("couldn't accept: {}", e),
                        };
                        modify_interest(fd, listener_fd, listener_read_event(100))?;
                    }
                    key => {
                        if let Some(stream) = contexts.get_mut(&key) {
                            let event: u32 = ev.events;
                            if (event as i32) & libc::EPOLLIN == libc::EPOLLIN {
                                let sent = stream.read_u128::<byteorder::LittleEndian>()?;

                                let now = SystemTime::now()
                                    .duration_since(SystemTime::UNIX_EPOCH)?
                                    .as_micros();

                                info!("Received Message Sent {}μs ago", now - sent);
                            } else {
                                println!("Unknown Event: {:?}", event);
                            }
                        }
                    }
                }
            }
        }
    });

    let client_handle = thread::spawn(|| -> Result<()> {
        loop {
            debug!("Client: Connecting");
            let mut stream = TcpStream::connect("127.0.0.1:8000")?;
            debug!("Client: Connected");
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_micros();

            let mut rng = rand::thread_rng();
            thread::sleep(Duration::from_micros(rng.gen_range(10..1000)));
            info!("Client: Sending");
            stream.write_u128::<byteorder::LittleEndian>(now)?;

            thread::sleep(Duration::from_secs(1));
        }
    });

    server_handle
        .join()
        .map_err(|e| anyhow!("Server Thread Error: {:?}", e))??;

    client_handle
        .join()
        .map_err(|e| anyhow!("client Thread Error: {:?}", e))??;

    Ok(())
}

fn add_interest(epoll_fd: RawFd, fd: RawFd, mut event: libc::epoll_event) -> Result<()> {
    syscall!(epoll_ctl(epoll_fd, libc::EPOLL_CTL_ADD, fd, &mut event))?;
    Ok(())
}

fn modify_interest(epoll_fd: RawFd, fd: RawFd, mut event: libc::epoll_event) -> Result<()> {
    syscall!(epoll_ctl(epoll_fd, libc::EPOLL_CTL_MOD, fd, &mut event))?;
    Ok(())
}

fn listener_read_event(key: u64) -> libc::epoll_event {
    libc::epoll_event {
        events: READ_FLAGS as u32,
        u64: key,
    }
}
