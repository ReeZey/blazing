mod http_data;
mod utils;

use std::{net::{TcpStream, TcpListener, SocketAddr}, io::{Write, BufReader, BufRead, Read}, path::{Path, PathBuf}, fs::{File, self}, collections::HashMap, println, time::{SystemTime, UNIX_EPOCH}};
use http_data::HTTPResponse;
use rand::{Rng, distributions::Alphanumeric};
use rhai::{Engine, packages::Package, Scope};
use rhai_fs::FilesystemPackage;
use config::Config;
use rusqlite::Connection;
use utils::{setup_db, format_error, format_response, send_respone, format_http_response};

#[tokio::main]
async fn main() {
    let config = Config::builder()
        .add_source(config::File::with_name("settings"))
        .add_source(config::Environment::with_prefix("APP"))
        .build()
        .unwrap();

    let public_path = config.get_string("root_location").unwrap();
    if !Path::new(&public_path).exists() {
        fs::create_dir(public_path).unwrap();
    }

    let enable_metrics = config.get_bool("enable_metrics").unwrap();
    if enable_metrics {
        let metric_location = config.get_string("metrics_location").unwrap();
        let conn = Connection::open(metric_location).unwrap();
        setup_db(&conn);
    }

    let binding_ip = config.get_string("ip").unwrap();
    let port = config.get_string("port").unwrap();
    let server = TcpListener::bind(format!("{binding_ip}:{port}")).expect("could not start server");

    loop {
        match server.accept() {
            Ok((stream, socket)) => {
                let config = config.clone();
                tokio::task::spawn(async move {
                    handle_connection(stream, socket, config);
                });
            },
            Err(e) => {
                println!("something bad happened idk err: {}", e.to_string());
            },
        }
        
    }
}

fn handle_connection(mut stream: TcpStream, socket: SocketAddr, config: Config) {
    let buf_reader = BufReader::new(&mut stream);
    let http_request: Vec<_> = buf_reader
        .lines()
        .filter_map(|result| result.ok()) // Filter out errors and unwrap Result values
        .take_while(|line| !line.is_empty()) // Handle empty lines
        .collect();

    //println!("{:#?}", http_request);

    if http_request.len() == 0 { return send_respone(&mut stream, format_error(204, "you send nothing?")); };
    
    //dont look at this, i dont like HTTP
    //thanks chatgpt
    let mut parts = http_request[0].splitn(2, ' ');
    let first_part = parts.next().unwrap();
    let last_part = parts.next().unwrap();
    let mut last_parts = last_part.rsplitn(2, ' ');
    let _http = last_parts.next().unwrap();
    let protocol = first_part.trim();

    let request_http = last_parts.next().unwrap().trim();

    let (http_path, entire_query) = match request_http.split_once("?") {
        Some(path_and_query) => {
            path_and_query
        }
        None => {
            (request_http, "")
        },
    };

    let mut query_params: HashMap<String, Option<String>> = HashMap::new();

    if entire_query.len() > 0 {
        for query in entire_query.split("&") {
            let (key, value) = match query.split_once("=") {
                Some((key, value)) => {
                    (urlencoding::decode(key).unwrap().to_string(), Some(urlencoding::decode(value).unwrap().to_string()))
                }
                None => {
                    (urlencoding::decode(query).unwrap().to_string(), None)
                }
            };

            if query_params.contains_key(&key) {
                return send_respone(&mut stream, format_error(400, "duplicate query parameter"));
            }

            query_params.insert(key.to_string(), value.clone());
        }
    }
    //println!("{:#?}", query_params);

    let mut request_headers: HashMap<String, String> = HashMap::new();
    for request in &http_request {
        match request.split_once(":") {
            Some((key, value)) => {
                if request_headers.contains_key(key) { 
                    return send_respone(&mut stream, format_error(400, "header existed twice"))
                };

                request_headers.insert(key.to_lowercase().to_owned(), value.trim_start().to_owned());
            }
            None => {}
        };
    };
    //println!("headers: {:#?}", request_headers);

    let root_location = config.get_string("root_location").unwrap();
    let file = urlencoding::decode(&http_path).unwrap().into_owned();
    let file_string = format!("{}{}", root_location, file);
    let mut file_path = Path::new(&file_string);

    let socket_ip = socket.ip().to_string();
    let ip = match request_headers.get("x-real-ip") {
        Some(ip) => ip,
        None => &socket_ip
    };

    println!("{} -> {} {}", ip, protocol, file);
    
    if config.get_bool("enable_metrics").unwrap() {
        let metric_location = config.get_string("metrics_location").unwrap();
        let conn = Connection::open(metric_location).unwrap();

        let user_agent = match request_headers.get("user-agent") {
            Some(user_agent) => user_agent,
            None => "unknown"
        };

        let unix_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis();

        conn.execute(
            "INSERT INTO metrics (location, user_agent, ip, date) VALUES (?1, ?2, ?3, ?4)",
            (&http_path, user_agent, &ip, unix_time as i64),
        ).unwrap();
    }

    match protocol {
        "GET" => {
            if !file_path.exists() {
                return send_respone(&mut stream, format_error(404, "file not found"));
            }
            if file_path.file_name().is_some() && file_path.file_name().unwrap() == ".hidden" {
                return send_respone(&mut stream, format_error(404, "file not found"));
            }
            
            let indexed_file = file_path.join("index.html");
            if file_path.is_dir() && file.ends_with("/") {
                if file_path.join(".hidden").exists() {
                    return send_respone(&mut stream, format_error(403, "access denied"));
                }

                if indexed_file.exists() {
                    file_path = &indexed_file;
                } else {
                    let parent_dir = indexed_file.parent().unwrap();

                    let mut template_html = fs::read_to_string(Path::new("template.html")).unwrap();
                    let mut build_html = String::new();
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

            if file_path.is_dir() {
                return send_respone(&mut stream, format_error(400, "expected file found folder"));
                //return redirect(&mut stream, "https://example.com");
            }

            /*
            TODO: compression
            */
            let extension = file_path.extension();
            if extension.is_some() {
                match extension.unwrap().to_string_lossy().to_lowercase().as_str() {
                    "rhai" => {
                        let mut engine = Engine::new();
        
                        let package = FilesystemPackage::new();
                        package.register_into_engine(&mut engine);

                        let mut scope = Scope::new();
                        scope.push_constant("hashmap", query_params.clone());

                        engine.register_fn("get", |hashmap: HashMap<String, Option<String>>, key: String| -> String {
                            if !hashmap.contains_key(&key) {
                                return "".to_owned();
                            }

                            let hashmap_value = hashmap.get(&key).unwrap().as_ref();
                            if hashmap_value.is_none() {
                                return "".to_owned();
                            }

                            return hashmap_value.unwrap().to_string();
                        });

                        let engine_exec = engine.eval_file_with_scope::<String>(&mut scope, file_path.to_path_buf());
                        
                        match engine_exec {
                            Ok(contents) => {
                                if query_params.clone().get("raw").is_some() {
                                    return send_respone(&mut stream, format_response(200, &contents));
                                }
                                return send_respone(&mut stream, format_http_response(200, &contents, "rhai execution"));
                            }
                            Err(e) => {
                                println!("err: {e}");
                                if query_params.clone().get("raw").is_some() {
                                    return send_respone(&mut stream, format_response(500, "error compile rhai"));
                                }
                                return send_respone(&mut stream, format_error(500, "error compile rhai"));
                            }
                        }
                    }
                    _ => {}
                }
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
            if config.get_bool("enable_uploads").unwrap() { 
                return send_respone(&mut stream, format_error(444, "not enabled"));
            };
            if http_path != "/upload" { 
                return send_respone(&mut stream, format_error(400, "where are you going?")); 
            };

            let mut file_length = match request_headers.get("content-length") {
                Some(string) => {
                    match string.parse::<usize>() {
                        Ok(number) => number,
                        Err(_) => 0,
                    }
                },
                None => 0,
            };

            if file_length == 0 { return send_respone(&mut stream, format_error(411, "length is zero")); }
            if file_length > 50_000_000 { return send_respone(&mut stream, format_response(413, "file is larger than 50mb")); }

            let file_id: String = rand::thread_rng()
                .sample_iter(&Alphanumeric)
                .take(8)
                .map(char::from)
                .collect();

            let directory = config.get_string("uploads_location").unwrap();
            let path = Path::new(&directory);

            if !path.exists() {
                fs::create_dir_all(path).expect("could not create paths for uploads_location");
            }

            let mut buffer = vec![0; 1_000_000];
            let mut count = stream.read(&mut buffer).unwrap();
            file_length -= count;

            //println!("{:?}", &buffer[0..20]);

            let extension = match infer::get(&buffer) {
                Some(mime) => mime.extension(),
                None => "bin",
            };
            
            let filename = format!("{file_id}.{extension}");
            let path_buf: PathBuf = path.join(&filename);
            let mut file = File::create(&path_buf).expect("could not create file");
            file.write_all(&buffer[0..count]).unwrap();

            while file_length > 0 {
                count = stream.read(&mut buffer).unwrap();
                if count == 0 { break; }
                file.write_all(&buffer[0..count]).unwrap();
                file_length -= count;
                //println!("{} nom {}", count, file_length);
            }
            
            file.flush().unwrap();
            return send_respone(&mut stream, format_response(200, &filename));
        },
        _ => {
            return send_respone(&mut stream, format_error(405, "mitä? 🇫🇮"))
        }
    }
}