#![no_std]
#![feature(async_fn_in_trait)]
#![allow(incomplete_features)]

use embassy_net::{tcp::TcpSocket, IpListenEndpoint};
use embedded_io::asynch::Write;
use httparse::Status;
use object_chain::{Chain, ChainElement, Link};

use crate::{
    handler::{Handler, Request},
    method::Method,
};

pub mod handler;
pub mod method;

pub struct BadServer<H: Handler, const REQUEST_BUFFER: usize, const MAX_HEADERS: usize> {
    handler: H,
}

impl<'s> BadServer<(), 1024, 32> {
    pub fn new() -> Self {
        Self { handler: () }
    }

    pub fn add_handler<H: Handler>(self, handler: H) -> BadServer<Chain<H>, 1024, 32> {
        BadServer {
            handler: Chain::new(handler),
        }
    }
}

impl<H, const REQUEST_BUFFER: usize, const MAX_HEADERS: usize>
    BadServer<Chain<H>, REQUEST_BUFFER, MAX_HEADERS>
where
    H: Handler,
{
    pub fn add_handler<H2: Handler>(
        self,
        handler: H2,
    ) -> BadServer<Link<H2, Chain<H>>, REQUEST_BUFFER, MAX_HEADERS> {
        BadServer {
            handler: self.handler.append(handler),
        }
    }
}

impl<H, P, const REQUEST_BUFFER: usize, const MAX_HEADERS: usize>
    BadServer<Link<H, P>, REQUEST_BUFFER, MAX_HEADERS>
where
    H: Handler,
    P: ChainElement + Handler,
{
    pub fn add_handler<H2: Handler>(
        self,
        handler: H2,
    ) -> BadServer<Link<H2, Link<H, P>>, REQUEST_BUFFER, MAX_HEADERS> {
        BadServer {
            handler: self.handler.append(handler),
        }
    }
}

impl<H, const REQUEST_BUFFER: usize, const MAX_HEADERS: usize>
    BadServer<H, REQUEST_BUFFER, MAX_HEADERS>
where
    H: Handler,
{
    pub fn with_buffer_size<const NEW_BUFFER_SIZE: usize>(
        self,
    ) -> BadServer<H, NEW_BUFFER_SIZE, MAX_HEADERS> {
        BadServer {
            handler: self.handler,
        }
    }

    pub fn with_header_count<const NEW_HEADER_COUNT: usize>(
        self,
    ) -> BadServer<H, REQUEST_BUFFER, NEW_HEADER_COUNT> {
        BadServer {
            handler: self.handler,
        }
    }

    pub async fn listen(&self, socket: &mut TcpSocket<'_>, port: u16) {
        loop {
            log::info!("Wait for connection");

            let r = socket.accept(IpListenEndpoint { addr: None, port }).await;

            log::info!("Connected");

            if let Err(e) = r {
                log::warn!("connect error: {:?}", e);
                continue;
            }

            self.handle(socket).await;
        }
    }

    async fn load_headers(
        &self,
        buffer: &mut [u8],
        socket: &mut TcpSocket<'_>,
    ) -> Result<(usize, usize), ()> {
        let mut pos = 0;
        while pos < buffer.len() {
            match socket.read(&mut buffer[pos..]).await {
                Ok(0) => {
                    // We're here because the previous read wasn't a complete request. Reading 0
                    // means the request will not ever be completed.
                    log::warn!("read EOF");
                    return Err(());
                }
                Ok(len) => pos += len,
                Err(e) => {
                    log::warn!("read error: {:?}", e);
                    return Err(());
                }
            }

            log::debug!("Buffer size: {pos}");

            let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
            let mut req = httparse::Request::new(&mut headers);

            match req.parse(&buffer[0..pos]) {
                Ok(Status::Complete(header_size)) => return Ok((header_size, pos)),
                Ok(Status::Partial) => {
                    // We need to read more
                }
                Err(_) => {
                    log::warn!("Parsing request failed");
                    return Err(());
                }
            };
        }

        // Can't read more, but we don't have a complete request yet.
        Err(())
    }

    async fn handle(&self, socket: &mut TcpSocket<'_>) {
        let mut buffer = [0u8; REQUEST_BUFFER];

        match self.load_headers(&mut buffer, socket).await {
            Ok((header_size, total_read)) => {
                let mut headers = [httparse::EMPTY_HEADER; MAX_HEADERS];
                let mut req = httparse::Request::new(&mut headers);

                let (header_buf, body_buf) = buffer.split_at_mut(header_size);
                req.parse(header_buf).unwrap();

                let Some(path) = req.path else {
                    log::warn!("Path not set");
                    // TODO: send a proper response
                    socket.close();
                    return;
                };

                let Some(method) = req.method.and_then(Method::new) else {
                    log::warn!("Unknown method: {:?}", req.method);
                    // TODO: send a proper response
                    socket.close();
                    return;
                };

                let request = Request {
                    method,
                    path,
                    body: b"",
                };
                if !self.handler.handles(&request) {
                    self.send_404(socket).await;
                    return;
                }

                // TODO: create a body reader that uses the loaded bytes,
                // and reads more from socket when needed.
                let read_body = total_read - header_size;
                let _buf = &body_buf[0..read_body];
                self.handler.handle(request).await;
            }
            Err(_) => todo!(),
        }
    }

    async fn send_404(&self, socket: &mut TcpSocket<'_>)
    where
        H: Handler,
    {
        // TODO: response builder
        let r = socket
            .write_all(
                b"HTTP/1.0 404 Not Found\r\n\r\n\
                        <html>\
                            <body>\
                                <h1>404 Not Found</h1>\
                            </body>\
                        </html>\r\n\
                        ",
            )
            .await;

        if let Err(e) = r {
            log::warn!("write error: {:?}", e);
        }

        if let Err(e) = socket.flush().await {
            log::warn!("flush error: {:?}", e);
        }
    }
}
