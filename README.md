# http_lolo

A simple, lightweight, single-threaded HTTP server with non-blocking I/O for concurrent operation handling.

## Features

- Single-threaded architecture with non-blocking I/O
- Concurrent request handling using epoll
- Simple and intuitive API for route handling
- Support for multiple server instances
- Customizable request and response handling

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
http_lolo = "0.1.0"
```

## Quick Start

```rust
use http_lolo::{HttpServer, Request, ResponseWriter};

fn main() {
    let server = HttpServer::new();

    server.handle_route("/", Box::new(|_: &mut Request, w: &mut ResponseWriter| {
        w.write_string("Hello, World!".to_string());
    }));

    server.listen_on("127.0.0.1:8080");

    HttpServer::run_all();
}
```

## Usage

### Creating a new server instance

```rust
let server = HttpServer::new();
```

### adding a route handler

```rust
server.handle_route("/", Box::new(|_: &mut Request, w: &mut ResponseWriter| {
    w.write_string("Hello, World!".to_string());
}));
```

### Starting the server

```rust
server.listen_on("127.0.0.1:8080");
HttpServer::run_all();
```

### Handling requests

Inside your route handler, you can access request data and write responses:

```rust
|req: &mut Request, resp: &mut ResponseWriter| {
    // Access request data
    println!("Method: {}", req.method);
    println!("Path: {}", req.path);

    // Write response
    resp.set_status(200);
    resp.write_string("Response content".to_string());
}
```

## Advanced Usage

### Multiple server instances

You can create multiple server instances listening on different ports:

```rust
let server1 = HttpServer::new();
let server2 = HttpServer::new();

// Add route handlers to server1 and server2

server1.listen_on("127.0.0.1:8080");
server2.listen_on("127.0.0.1:8081");

HttpServer::run_all();
```
### JSON responses

```rust
use http_lolo::JsonValue;

resp.write_json(json::object!{
    "key" => "value",
    "number" => 42
});
```
## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.
