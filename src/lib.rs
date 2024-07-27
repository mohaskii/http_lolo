use std::collections::HashMap;
use std::net::TcpListener;
use std::str;
use std::sync::Mutex;
mod http_status;
pub use http_status::*;
pub use json::*;
use lazy_static::lazy_static;
use std::os::unix::io::AsRawFd;
mod request;
use request::*;
mod utils;
use utils::*;
mod response_writer;
use response_writer::*;

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

#[derive(Debug, Clone)]
pub struct HttpServer {
    server_id: ServerId,
}
pub type EventId = usize;
pub type ServerId = usize;

lazy_static! {
    static ref ROUTES: Mutex<HashMap<String, Handler>> = Mutex::new(HashMap::new());
    static ref REQUEST_CTX: Mutex<HashMap<EventId, Request>> = Mutex::new(HashMap::new());
    static ref WRITE_CTX: Mutex<HashMap<EventId, ResponseWriter>> = Mutex::new(HashMap::new());
    static ref SERVER_ID: Mutex<EventId> = Mutex::new(100);
    static ref SERVER_CTX: Mutex<HashMap<ServerId, (TcpListener, i32)>> =
        Mutex::new(HashMap::new());
    static ref EPOLL_FD: i32 = epoll_create().expect("can create epoll queue");
    static ref EVENTS: Mutex<Vec<libc::epoll_event>> = Mutex::new(Vec::with_capacity(1024));
}

type Handler = Box<dyn Fn(&mut Request, &mut ResponseWriter) + Send + Sync>;

impl HttpServer {
    pub fn new() -> Self {
        let mut server_id_guard = SERVER_ID.lock().unwrap();
        let server_id = server_id_guard.clone();
        *server_id_guard += 1;
        HttpServer { server_id }
    }

    pub fn run_all() {
        loop {
            EVENTS.lock().unwrap().clear();
            let res = match syscall!(epoll_wait(
                *EPOLL_FD,
                EVENTS.lock().unwrap().as_mut_ptr() as *mut libc::epoll_event,
                1024,
                1000 as libc::c_int,
            )) {
                Ok(v) => v,
                Err(e) => panic!("error during epoll wait: {}", e),
            };

            // safe  as long as the kernel does nothing wrong - copied from mio
            unsafe { EVENTS.lock().unwrap().set_len(res as usize) };

            for ev in EVENTS.lock().unwrap().iter() {
                match SERVER_CTX.lock().unwrap().get(&(ev.u64 as usize)) {
                    Some((listerner, listener_fd)) => {
                        match listerner.accept() {
                            Ok((stream, addr)) => {
                                stream.set_nonblocking(true).unwrap();
                                println!("new client on server {}: {}", ev.u64 as usize, addr);
                                let mut request_contexts = REQUEST_CTX.lock().unwrap();
                                let key = SERVER_ID.lock().unwrap().clone();
                                add_interest(
                                    *EPOLL_FD,
                                    stream.as_raw_fd(),
                                    listener_read_event(key as u64),
                                )
                                .unwrap();

                                request_contexts.insert(key, Request::new(stream, ev.u64 as usize));
                                *SERVER_ID.lock().unwrap() += 1;
                            }
                            Err(e) => {
                                eprintln!("couldn't accept on server {}: {}", ev.u64 as usize, e)
                            }
                        };
                        modify_interest(
                            *EPOLL_FD,
                            listener_fd.clone(),
                            listener_read_event(ev.u64 as u64),
                        )
                        .unwrap();
                    }
                    None => {
                        let key = ev.u64 as usize;
                        let mut to_delete = None;
                        if let Some(context) = REQUEST_CTX.lock().unwrap().get_mut(&key) {
                            let events: u32 = ev.events;
                            match events {
                                v if v as i32 & libc::EPOLLIN == libc::EPOLLIN => {
                                    context.read_cb(key, *EPOLL_FD).unwrap();
                                }
                                v if v as i32 & libc::EPOLLOUT == libc::EPOLLOUT => {
                                    context.write_cb(key);
                                    to_delete = Some(key);
                                }
                                v => println!("unexpected events: {}", v),
                            };
                        }
                        if let Some(key) = to_delete {
                            REQUEST_CTX.lock().unwrap().remove(&key);
                        }
                        continue;
                    }
                }
            }
        }
    }
    pub fn handle_route(&self, path: &str, handler: Handler) {
        let mut routes = ROUTES.lock().unwrap();
        routes.insert(
            path.to_string() + "." + self.server_id.to_string().as_str(),
            handler,
        );
    }

    pub fn listen_on(&self, addr: &str) {
        // let epoll_fd = epoll_create().expect("can create epoll queue");
        let listener = TcpListener::bind(addr).unwrap();
        listener.set_nonblocking(true).expect("nonblocking works");
        let listener_fd = listener.as_raw_fd();
        add_interest(
            *EPOLL_FD,
            listener_fd,
            listener_read_event(self.server_id as u64),
        )
        .unwrap();
        SERVER_CTX
            .lock()
            .unwrap()
            .insert(self.server_id, (listener, listener_fd));
    }
}
