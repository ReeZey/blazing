mod http_data;

use std::{net::{TcpStream, TcpListener, SocketAddr}, io::{Write, BufReader, BufRead, Read}, path::{Path, PathBuf}, fs::{File, self}, collections::HashMap};
use http_data::HTTPResponse;
use rand::{Rng, distributions::Alphanumeric};
use rhai::{Engine, packages::Package};
use rhai_fs::FilesystemPackage;
use config::Config;

fn main() {
    let settings = Config::builder()
        .add_source(config::File::with_name("settings"))
        .add_source(config::Environment::with_prefix("APP"))
        .build()
        .unwrap();

    let config: HashMap<String, String> = settings.try_deserialize::<HashMap<String, String>>().unwrap();
    let public = config.get("root_location").expect("can't find root location  variable");

    if !Path::new(public).exists() {
        fs::create_dir(public).unwrap();
    }
    let binding_ip = get_config(&config, "ip");
    let port = get_config(&config, "port");
    let server = TcpListener::bind(format!("{binding_ip}:{port}")).expect("could not start server");

    loop {
        match server.accept() {
            Ok((stream, socket)) => {
                handle_connection(stream, socket, config.clone());
            },
            Err(e) => {
                println!("something bad happened idk err: {}", e.to_string());
            },
        }
    }
}

fn handle_connection(mut stream: TcpStream, socket: SocketAddr, config: HashMap<String, String>) {
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

    let [protocol, http_path, _http]: [&str; 3] = stuffs.try_into().unwrap();

    let mut request_headers: HashMap<String, String> = HashMap::new();
    for request in &http_request {
        let req = request.split(":").collect::<Vec<&str>>();
        if req.len() != 2 { continue; }
        if request_headers.contains_key(req[0]) { return send_respone(&mut stream, format_response(400, "header existed twice"))};

        request_headers.insert(req[0].to_lowercase().to_owned(), req[1].trim_start().to_owned());
    }
    //println!("{:#?}", request_data);

    let root_location = get_config(&config, "root_location");
    let file = urlencoding::decode(&http_path).unwrap().into_owned();
    let file_string = format!("{}{}", root_location, file);
    let mut file_path = Path::new(&file_string);

    let socket_ip = socket.ip().to_string();
    let ip = if let Some(header_ip) = request_headers.get("x-real-ip") {
        header_ip
    }else {
        &socket_ip
    };

    println!("{} -> {} {}", ip, protocol, file);

    match protocol {
        "GET" => {
            if file.len() > 0 && file_path.is_dir() && !file.ends_with("/") {
                return redirect(&mut stream, &format!("{}/", &file));
            }
            
            let indexed_file = file_path.join("index.html");
            if file_path.is_dir() {
                if indexed_file.exists() {
                    file_path = &indexed_file;
                }else {
                    let parent_dir = indexed_file.parent().unwrap();
        
                    if !parent_dir.exists() {
                        return send_respone(&mut stream, format_response(404, "folder not found"));
                    }

                    let mut template_html = fs::read_to_string(Path::new("template.html")).unwrap();
                    let mut build_html = "".to_string();
                    for dir in parent_dir.read_dir().unwrap() {
                        let directior = dir.unwrap();
        
                        let path = directior.path();
                        let mut name = path.file_name().unwrap().to_string_lossy();
                        if path.is_dir() {
                            name += "/";
                        }
                        build_html += &format!("<a href='{0}'>{0}</a> ", name);
                    }
                    template_html = template_html.replace("{title}", &file);
                    template_html = template_html.replace("{content}", &build_html);
                    return send_respone(&mut stream, format_response(200, &template_html));
                }
            }
        
            if !file_path.exists() {
                return send_respone(&mut stream, format_response(404, "file not found"));
            }
        
            match file_path.extension() {
                Some(ext) => {
                    if ext == "rhai" {
                        let mut engine = Engine::new();
        
                        let package = FilesystemPackage::new();
                        package.register_into_engine(&mut engine);

                        engine.register_fn("hello", |who: String| -> String {
                            println!("hello {}", who);

                            return format!("you are welcome {who}!");
                        });
        
                        let engine_exec = engine.eval_file::<String>(file_path.to_path_buf());
                        
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
            let mut file = File::open(&file_path).unwrap();
            file.read_to_end(&mut buffer).unwrap();
        
            let mut http_response = HTTPResponse::default();
            http_response.buffer = buffer;
        
            send_respone(&mut stream, http_response);
        },
        "PUT" => {
            //get_config
            if get_config(&config, "enable_uploads") != "true" { return send_respone(&mut stream, format_response(444, "not enabled")); };
            if http_path != "/upload" { return send_respone(&mut stream, format_response(400, "va?")); };

            let file_length = match request_headers.get("content-length") {
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

            let mut buffer = vec![0; file_length];
            let try_read = stream.read_exact(&mut buffer);
            if try_read.is_err() {
                //println!("{:#?}", buffer);
                println!("transfer error: {}", try_read.unwrap_err());
                return send_respone(&mut stream, format_response(200, "hejsan"));
            }

            let extension = match infer::get(&buffer) {
                Some(mime) => mime.extension(),
                None => "bin",
            };

            let filename: String = rand::thread_rng()
                                .sample_iter(&Alphanumeric)
                                .take(8)
                                .map(char::from)
                                .collect();

            let directory = get_config(&config, "uploads_location");
            let path = Path::new(&directory);

            if !path.exists() {
                fs::create_dir_all(path).expect("could not create paths for uploads_location");
            }

            let path_buf: PathBuf = path.join(format!("{filename}.{extension}"));
            let mut file = File::create(&path_buf).expect("could not create file");

            file.write_all(&mut buffer).expect("could not write file");

            return send_respone(&mut stream, format_response(200, &path_buf.as_path().to_str().unwrap()));
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
    if stream.write_all(&data.format()).is_err() {
        drop(stream);
    }
}

fn get_config(config: &HashMap<String, String>, key: &str) -> String {
    return config.get(&key.to_owned()).expect(&format!("could not find config key {}, did you possibly delete it?", key)).to_owned();
}