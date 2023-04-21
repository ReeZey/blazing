use std::time::SystemTime;

pub struct HTTPResponse {
    pub status: i32,
    pub server: String,
    pub date: String,
    pub last_modified: Option<String>,
    pub content_type: Option<String>,
    pub buffer: Vec<u8>
}

impl Default for HTTPResponse {
    fn default() -> Self {
        Self {
            status: 200,
            server: "blazing fast".to_owned(), 
            date: httpdate::fmt_http_date(SystemTime::now()),
            last_modified: None,
            content_type: None,
            buffer: vec![]
        }
    }
}

impl HTTPResponse {
    pub fn format(self) -> Vec<u8> {
        let mut http_headers = "HTTP/1.1 ".to_owned();
        http_headers += &format!("{}\r\n", self.status);
        http_headers += &format!("Content-Length: {}\r\n", self.buffer.len());
        http_headers += &format!("Server: {}\r\n", self.server);
        http_headers += &format!("Date: {}\r\n", self.date);
        if self.last_modified.is_some() { http_headers += &format!("Last_modified: {}\r\n", self.last_modified.unwrap()) };
        if self.content_type.is_some() {
            http_headers += &format!("Content-Type: {}\r\n", self.content_type.unwrap()) 
        }else {
            let mime = infer::get(&self.buffer);
            if mime.is_some() {
                http_headers += &format!("Content-Type: {}\r\n", mime.unwrap().mime_type());
            }
        }

        http_headers += &"\r\n".to_owned();

        let mut buffer = http_headers.as_bytes().to_vec();
        buffer.extend(self.buffer);

        return buffer;
    }
}