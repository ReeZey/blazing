mod http_data;

use std::{net::{TcpStream, TcpListener, SocketAddr}, io::{Write, BufReader, BufRead, Read}, path::Path, fs::{File, self}};
use crate::http_data::HTTPResponse;
use rhai::{Engine, packages::Package};
use rhai_fs::FilesystemPackage;

fn main() {
    if !Path::new("public").exists() {
        fs::create_dir("public").unwrap();
    }
    
    let server = TcpListener::bind("0.0.0.0:51413").expect("could not start server");

    loop {
        match server.accept() {
            Ok((stream, socket)) => {
                handle_connection(stream, socket);
            },
            Err(e) => {
                println!("something bad happened idk err: {}", e.to_string());
            },
        }
    }
}

fn handle_connection(mut stream: TcpStream, socket: SocketAddr) {
    let buf_reader = BufReader::new(&mut stream);
    let http_request: Vec<_> = buf_reader
        .lines()
        .filter_map(|result| result.ok()) // Filter out errors and unwrap Result values
        .take_while(|line| !line.is_empty()) // Handle empty lines
        .collect();

    if http_request.len() == 0 { return send_respone(&mut stream, format_response(418, "why u send no data 🤨")); };
    let stuffs = http_request[0].split(" ").collect::<Vec<&str>>();
    if stuffs.len() != 3 { return send_respone(&mut stream, format_response(418, "what are u doing 🤨")); }
    
    let [protocol, path, _http]: [&str; 3] = stuffs.try_into().unwrap();
    let mut file = path.to_string();
    
    //println!("{:?}", http_request);
    
    if protocol != "GET" { 
        return send_respone(&mut stream, format_response(400, "i dont speak this protocol, i only speak get"));
    }

    if !file.ends_with("/") && Path::new(&format!("public{}", &file)).is_dir() {
        file += "/";
        return redirect(&mut stream, &file);
    }

    let stringpath = format!("public{}", &file);
    println!("{} -> {}", socket.ip(), stringpath);
    let mut filepath = Path::new(&stringpath);

    let indexed_file = filepath.join("index.html");
    if filepath.is_dir() {
        if indexed_file.exists() {
            filepath = &indexed_file;
        }else {
            let parent_dir = indexed_file.parent().unwrap();

            if !parent_dir.exists() {
                return send_respone(&mut stream, format_response(404, "folder not found"));
            }

            let mut build_html: String = "<!DOCTYPE html>\r\n".to_owned();
            build_html += "<a href='../'>..</a>\r\n";
            for dir in parent_dir.read_dir().unwrap() {
                let directior = dir.unwrap();

                let path = directior.path();
                let mut name = path.file_name().unwrap().to_string_lossy();
                if path.is_dir() {
                    name += "/";
                }
                build_html += &format!("<a href='{0}'>{0}</a> ", name);
            }
            return send_respone(&mut stream, format_response(200, &build_html));
        }
    }

    if !filepath.exists() {
        return send_respone(&mut stream, format_response(404, "file not found"));
    }

    match filepath.extension() {
        Some(ext) => {
            if ext == "rhai" {
                let mut engine = Engine::new();

                // Create filesystem package and add the package into the engine
                let package = FilesystemPackage::new();
                package.register_into_engine(&mut engine);


                // Print the contents of the file `Cargo.toml`.
                let engine_exec = engine.eval_file::<String>(filepath.to_path_buf());
                
                match engine_exec {
                    Ok(contents) => {
                        return send_respone(&mut stream, format_response(200, &contents));
                    }
                    Err(e) => {
                        println!("err: {e}");
                        return send_respone(&mut stream, format_response(500, "error compile"));
                    }
                }
            }
        }
        None => {}
    }

    let mut buffer = vec![];
    let mut file = File::open(&filepath).unwrap();
    file.read_to_end(&mut buffer).unwrap();

    let mut http_response = HTTPResponse::default();
    http_response.buffer = buffer;

    send_respone(&mut stream, http_response);
}

fn redirect(stream: &mut TcpStream, url: &str) {
    stream.write_all(&format!("HTTP/1.1 302\r\nLocation: {}", url).as_bytes().to_vec()).unwrap();
}

fn format_response(status: i32, response: &str) -> HTTPResponse {
    let mut http_response = HTTPResponse::default();
    http_response.status = status;
    http_response.buffer = response.as_bytes().to_vec();
    return http_response;
}

fn send_respone(stream: &mut TcpStream, data: HTTPResponse) {
    stream.write_all(&data.format()).unwrap();
}
