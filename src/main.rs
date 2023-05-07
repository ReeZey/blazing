mod http_data;

use std::{net::{TcpStream, TcpListener, SocketAddr}, io::{Write, BufReader, BufRead, Read}, path::{Path, PathBuf}, fs::{File, self}, collections::HashMap};
use http_data::HTTPResponse;
use rand::{Rng, distributions::Alphanumeric};
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

    //println!("{:#?}", http_request);

    if http_request.len() == 0 { return send_respone(&mut stream, format_response(204, "")); };
    let stuffs: Vec<&str> = http_request[0].split(" ").collect();
    if stuffs.len() != 3 { return send_respone(&mut stream, format_response(400, "wrongful usage majj")); };

    let [protocol, path, _http]: [&str; 3] = stuffs.try_into().unwrap();

    let mut request_data: HashMap<String, String> = HashMap::new();
    for request in &http_request {
        let req = request.split(":").collect::<Vec<&str>>();
        if req.len() != 2 { continue; }
        if request_data.contains_key(req[0]) { return send_respone(&mut stream, format_response(400, "key existed twice"))};

        request_data.insert(req[0].to_lowercase().to_owned(), req[1].trim_start().to_owned());
    }

    //println!("{:#?}", request_data);
    let mut file = path.to_string();
    let public = Path::new("public");
    match protocol {
        "GET" => {
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
        
                        let package = FilesystemPackage::new();
                        package.register_into_engine(&mut engine);

                        engine.register_fn("hello", |who: String| -> String {
                            println!("hello {}", who);

                            return format!("you are welcome {who}!");
                        });
        
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
        },
        "PUT" => {
            let file_length = match request_data.get("content-length") {
                Some(string) => {
                    match string.parse::<usize>() {
                        Ok(number) => number,
                        Err(_) => 0,
                    }
                },
                None => 0,
            };

            if file_length == 0 { return send_respone(&mut stream, format_response(411, "length is zero")); }
            if file_length > 50_000_000 { return send_respone(&mut stream, format_response(413, "file is larger than 50mb")); }

            println!("lenght: {}", file_length);

            let mut buffer = vec![0; file_length];
            stream.read_exact(&mut buffer).expect("could not read buffer");

            let extension = match infer::get(&buffer) {
                Some(mime) => mime.extension(),
                None => "bin",
            };

            let filename: String = rand::thread_rng()
                                .sample_iter(&Alphanumeric)
                                .take(8)
                                .map(char::from)
                                .collect();

            
            let path: PathBuf = PathBuf::new().join("upload").join(format!("{filename}.{extension}"));
            let mut file = File::create(public.join(&path)).expect("could not create file");

            file.write_all(&mut buffer).expect("could not write file");

            return send_respone(&mut stream, format_response(200, &path.as_path().to_str().unwrap()));
        },
        _ => {}
    }
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
