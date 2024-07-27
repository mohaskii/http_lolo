use crate::{Request, ResponseWriter, ServerError};
use std::env;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub fn handle_cgi(
    req: &Request,
    resp: &mut ResponseWriter,
    interpreter: &Path,
    script_path: &Path,
) {
    // Get the current working directory
    let cwd = env::current_dir()
        .map_err(|e| ServerError(format!("Failed to get current directory: {}", e)))
        .unwrap();
    // Construct the full path to the script
    let full_script_path = cwd.join(script_path.strip_prefix("/").unwrap_or(script_path));
    println!("Executing CGI script");
    let mut command = Command::new(interpreter);
    command.arg(&full_script_path);
    // Set up environment variables
    command.env("REQUEST_METHOD", &req.method);
    command.env("PATH_INFO", &req.path);
    command.env("QUERY_STRING", req.path.split('?').nth(1).unwrap_or(""));
    for (key, value) in &req.headers {
        let env_key = format!("HTTP_{}", key.to_uppercase().replace("-", "_"));
        command.env(env_key, value);
    }
    // Set up stdin, stdout, and stderr
    command.stdin(Stdio::piped());
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());
    // Run the CGI script
    let mut child = command
        .spawn()
        .map_err(|e| ServerError(format!("Failed to spawn CGI process: {}", e)))
        .unwrap();
    // Write request body to CGI script's stdin if present
    let mut stdin = child.stdin.take().unwrap();
    stdin
        .write_all(&req.body)
        .map_err(|e| ServerError(format!("Failed to write to CGI stdin: {}", e)))
        .unwrap();
    // Read CGI script's stdout
    let output = child
        .wait_with_output()
        .map_err(|e| ServerError(format!("Failed to read CGI output: {}", e)))
        .unwrap();
    // Parse CGI output and set response
    let cgi_output = String::from_utf8_lossy(&output.stdout);
    let mut headers_end = false;
    for line in cgi_output.lines() {
        if line.is_empty() {
            headers_end = true;
            continue;
        }
        if !headers_end {
            let parts: Vec<&str> = line.splitn(2, ':').collect();
            if parts.len() == 2 {
                resp.headers
                    .entry(parts[0].trim().to_string())
                    .or_insert_with(Vec::new)
                    .push(parts[1].trim().to_string());
            }
        } else {
            resp.body.extend_from_slice(line.as_bytes());
            resp.body.extend_from_slice(b"\n");
        }
    }
    if !output.status.success() {
        println!(
            "CGI script error: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        resp.write_status(crate::HttpStatus::NotFound);
        return;
    }
    resp.write();
}