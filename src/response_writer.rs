pub use http_status::*;
pub use json::*;
use std::collections::HashMap;
use std::io::{self, Write};
use std::net::TcpStream;
use std::path::Path;
use std::{fs, str};

use crate::{ http_status, remove_interest, EventId, EPOLL_FD, WRITE_CTX};
#[derive(Debug)]
pub struct ResponseWriter {
    event_id: EventId,
    pub body: Vec<u8>,
    pub stream: TcpStream,
    pub headers: HashMap<String, Vec<String>>,
    pub status_code: Option<u16>,
}
impl ResponseWriter {
    pub fn new(stream: TcpStream, event_fd_id: EventId) -> Self {
        ResponseWriter {
            event_id: event_fd_id,
            body: Vec::new(),
            stream,
            headers: HashMap::new(),
            status_code: None,
        }
    }
    pub fn execute_html_file(&mut self, file_path: &str) -> io::Result<()> {
        let path = Path::new(file_path);
        if path.exists() && path.is_file() {
            let contents = fs::read_to_string(path)?;
            self.headers
                .insert("Content-Type".to_string(), vec!["text/html".to_string()]);
            self.body = contents.into_bytes();
            self.write();
            Ok(())
        } else {
            self.set_status(404);
            self.write_string("File not found");
            Err(io::Error::new(io::ErrorKind::NotFound, "File not found"))
        }
    }
    fn clone(&self) -> Self {
        ResponseWriter {
            body: self.body.clone(),
            stream: self.stream.try_clone().unwrap(),
            headers: self.headers.clone(),
            status_code: self.status_code.clone(),
            event_id: self.event_id.clone(),
        }
    }
    pub fn write_string(&mut self, str: &str) {
        self.body.extend_from_slice(str.as_bytes());
        self.write()
    }   

    pub fn set_headers(&mut self, object: JsonValue) {
        for (key, value) in object.entries() {
            self.headers
                .insert(key.to_string(), vec![value.to_string()]);
        }
    }
    fn write(&self) {
        let v = self.clone();
        WRITE_CTX.lock().unwrap().insert(self.event_id, v);
    }
    pub fn excute(&mut self, og_raw_fd: i32) {
        let mut headers_str = String::new();
        for (k, v) in &self.headers {
            for header_value in v {
                headers_str.push_str(&format!("{}: {}\r\n", k, header_value));
            }
        }
        let status = HttpStatus::from_code(self.status_code.or(Some(200)).unwrap());
        let response = format!(
            "HTTP/1.1 {} {}\r\n{}\r\n",
            status.code(),
            status.reason_phrase(),
            headers_str,
        );

        let mut response_extended = response.as_bytes().to_vec();
        response_extended.extend_from_slice(&self.body);

        self.stream.write_all(response_extended.as_slice()).unwrap();

        let _ = self.stream.shutdown(std::net::Shutdown::Both);

        remove_interest(*EPOLL_FD, og_raw_fd).unwrap();
    }
    pub fn set_cookie(&mut self, name: &str, value: &str) {
        self.headers
            .entry("Set-Cookie".to_string())
            .or_insert_with(Vec::new)
            .push(format!("{}={}", name, value));
    }

    pub fn set_cookie_with_attributes(
        &mut self,
        name: &str,
        value: &str,
        attributes: &[(&str, Option<&str>)],
    ) {
        let mut cookie = format!("{}={}", name, value);
        for (attr, attr_value) in attributes {
            match attr_value {
                Some(val) => cookie.push_str(&format!("; {}={}", attr, val)),
                None => cookie.push_str(&format!("; {}", attr)),
            }
        }
        self.headers
            .entry("Set-Cookie".to_string())
            .or_insert_with(Vec::new)
            .push(cookie);
    }
    pub fn write_json(&mut self, body: JsonValue) {
        self.headers.insert(
            "Content-Type".to_string(),
            vec!["application/json".to_string()],
        );
        self.body = body.dump().as_bytes().to_vec();
        self.write()
    }

    pub fn set_status(&mut self, status_code: u16) {
        self.status_code = Some(status_code);
    }

    pub fn write_status(&mut self, status: HttpStatus) {
        self.body = status.reason_phrase().as_bytes().to_vec();
        self.set_status(status.code());
        self.write();
    }
}
