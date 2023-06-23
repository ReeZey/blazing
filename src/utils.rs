
/*
TODO: fix brokie
fn redirect(stream: &mut TcpStream, url: &str) {
    println!("{}", url);
    stream.write_all(&format!("HTTP/1.1 301 Moved Permanently\r\nLocation: {}", url).as_bytes().to_vec()).unwrap();
}
*/

use std::{fs, path::Path, net::TcpStream, io::Write};

use rusqlite::Connection;

use crate::http_data::HTTPResponse;

pub(crate) fn format_response(status: i32, response: &str) -> HTTPResponse {
    let mut http_response = HTTPResponse::default();
    http_response.status = status;
    http_response.buffer = response.as_bytes().to_vec();
    return http_response;
}

pub(crate) fn send_respone(stream: &mut TcpStream, data: HTTPResponse) {
    if stream.write_all(&data.format()).is_err() {
        drop(stream);
    }
}

pub(crate) fn format_error(status: i32, response: &str) -> HTTPResponse {
    let status_string = status.to_string();

    let mut template_html = fs::read_to_string(Path::new("template.html")).unwrap();
    template_html = template_html.replace("{title}", response);

    let mut build_html = format!("<p class='header'>error {}<br>{}</p>", status_string, response);
    build_html += &format!("<img src='https://cats.reez.it/{}' style='max-width: 100%;'></img>", status_string);

    template_html = template_html.replace("{content}", &build_html);
    return format_response(status, &template_html);
}

#[allow(dead_code)]
pub(crate) fn format_http_response(status: i32, response: &str) -> HTTPResponse {
    let mut template_html = fs::read_to_string(Path::new("template.html")).unwrap();
    template_html = template_html.replace("{title}", response);
    template_html = template_html.replace("{content}", response);
    return format_response(status, &template_html);
}

pub(crate) fn setup_db(conn: &Connection) {
    conn.execute("CREATE TABLE IF NOT EXISTS metrics (
            id INTEGER UNIQUE,
            location TEXT,
            user_agent TEXT,
            ip TEXT,
            date DATETIME,
            PRIMARY KEY(id AUTOINCREMENT)
        )",
        (), // empty list of parameters.
    ).unwrap();
}