use http_lolo::{HttpServer, HttpStatus, Request, ResponseWriter};

fn hello_handler(r: &mut Request, w: &mut ResponseWriter) {
    if r.method != "GET" {
        w.write_status(HttpStatus::MethodNotAllowed);
    }
    w.write_string("helolo")
}

fn main() {
    let my_server = HttpServer::new();
    let my_server1 = HttpServer::new();
    let my_server2 = HttpServer::new();

    my_server.handle_route(
        "/",
        Box::new(|_: &mut Request, w: &mut ResponseWriter| {
            w.set_cookie("session_id", "12345");
            w.set_cookie_with_attributes(
                "user",
                "john_doe",
                &[
                    ("Path", Some("/")),
                    ("HttpOnly", None),
                    ("Max-Age", Some("3600")),
                ],
            );
            w.write_string("cookie are set");
        }),
    );

    my_server1.handle_route("/", Box::new(hello_handler));
    my_server2.handle_route("/helo", Box::new(hello_handler));

    my_server.listen_on("127.0.0.1:8080");
    my_server1.listen_on("127.0.0.1:8082");
    my_server2.listen_on("127.0.0.1:8083");

    HttpServer::run_all();
}
