use std::collections::HashMap;
use std::io::{self, Cursor, ErrorKind, Read};
use std::net::TcpStream;
use multipart::server::Multipart;
use std::os::unix::io::AsRawFd;
use crate::ServerId;
use crate::{utils::*, EventId, HttpStatus, ResponseWriter, ROUTES, WRITE_CTX};

pub struct Request {
    header_done: bool,
    server_id: ServerId,
    pub stream: TcpStream,
    pub method: String,
    pub path: String,
    pub body: Vec<u8>,
    pub protocol: String,
    pub headers: HashMap<String, String>,
    pub cookies: HashMap<String, String>,
}

impl Request {
    pub fn new(stream: TcpStream, server_id: ServerId) -> Self {
        Self {
            cookies: HashMap::new(),
            header_done: false,
            body: Vec::new(),
            server_id,
            stream,
            headers: HashMap::new(),
            path: String::default(),
            protocol: String::default(),
            method: String::default(),
        }
    }
    pub fn parse_multipart(&mut self) -> Option<HashMap<String, Vec<u8>>> {
        let boundary = self.get_boundary()?;
        let mut multipart = Multipart::with_body(Cursor::new(&self.body), boundary);

        let mut form_data = HashMap::new();

        loop {
            match multipart.read_entry() {
                Ok(Some(mut field)) => {
                    let mut data = Vec::new();
                    if field.data.read_to_end(&mut data).is_ok() {
                        let filename = field
                            .headers
                            .filename
                            .clone()
                            .unwrap_or_else(|| field.headers.name.clone().to_string())
                            .to_string(); // Convertir Arc<str> en String
                        form_data.insert(filename, data);
                    }
                }
                Ok(None) => break,
                Err(_) => return None,
            }
        }

        Some(form_data)
    }
    fn get_boundary(&self) -> Option<String> {
        let content_type = self.headers.get("Content-Type")?;

        content_type.split(';').find_map(|part| {
            let part = part.trim();
            if part.starts_with("boundary=") {
                Some(part[9..].trim_matches('"').to_string())
            } else {
                None
            }
        })
    }
    pub fn read_cb(&mut self, event_id: EventId, epoll_fd: i32) -> io::Result<()> {
        let mut buffer = Vec::new();
        let mut temp_buf = [0u8; 4096];

        match self.stream.read(&mut temp_buf) {
            Ok(n) => {
                buffer.extend_from_slice(&temp_buf[..n]);
            }
            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                return Ok(());
            }
            Err(e) => return Err(e),
        }

        if !buffer.is_empty() {
            if !self.header_done {
                self.parse_request(&buffer);
            } else {
                self.body.extend_from_slice(&buffer);
            }

            if self.header_done && self.body.len() >= self.content_length() {
                self.handle_complete_request(event_id, epoll_fd)?;
            } else {
                modify_interest(
                    epoll_fd,
                    self.stream.as_raw_fd(),
                    listener_read_event(event_id as u64),
                )?;
            }
        }

        Ok(())
    }

    fn handle_complete_request(&mut self, event_id: EventId, epoll_fd: i32) -> io::Result<()> {
        let stream_clone = self.stream.try_clone()?;
        let mut response_writer = ResponseWriter::new(stream_clone, event_id);

        match ROUTES
            .lock()
            .unwrap()
            .get(&(self.path.clone() + "." + self.server_id.to_string().as_str()))
        {
            Some(handler) => {
                handler(self, &mut response_writer);
            }
            None => {
                response_writer.write_status(HttpStatus::NotFound);
            }
        }

        modify_interest(
            epoll_fd,
            self.stream.as_raw_fd(),
            listener_write_event(event_id as u64),
        )
    }
    pub fn write_cb(&self, event_id: EventId) {
        if let Some(ctx) = WRITE_CTX
            .lock()
            .expect("can lock request_contexts")
            .get_mut(&event_id)
        {
            ctx.excute(self.stream.as_raw_fd())
        }

        WRITE_CTX
            .lock()
            .expect("can lock request contexts")
            .remove(&event_id);
    }

    fn parse_request(&mut self, data: &[u8]) {
        let mut headers = [httparse::EMPTY_HEADER; 64];
        let mut req = httparse::Request::new(&mut headers);

        match req.parse(data) {
            Ok(status) => {
                if let httparse::Status::Complete(headers_len) = status {
                    self.method = req.method.unwrap_or("").to_string();
                    self.path = req.path.unwrap_or("").to_string();
                    self.protocol = req
                        .version
                        .map(|v| format!("HTTP/1.{}", v))
                        .unwrap_or_default();

                    for header in req.headers.iter() {
                        if let Ok(value) = std::str::from_utf8(header.value) {
                            self.headers
                                .insert(header.name.to_string(), value.to_string());
                        }
                    }

                    if let Some(cookie_header) = self.headers.get("Cookie") {
                        for cookie in cookie_header.split(';') {
                            if let Some((key, value)) = cookie.trim().split_once('=') {
                                self.cookies.insert(key.to_string(), value.to_string());
                            }
                        }
                    }

                    self.header_done = true;

                    // Ajout du corps si présent
                    if headers_len < data.len() {
                        self.body.extend_from_slice(&data[headers_len..]);
                    }
                } else {
                    // Si l'analyse n'est pas complète, on stocke les données partielles
                    self.body.extend_from_slice(data);
                }
            }
            Err(e) => {
                eprintln!("Erreur lors de l'analyse de la requête : {:?}", e);
            }
        }
    }

    fn content_length(&self) -> usize {
        self.headers
            .get("Content-Length")
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0)
    }
}
