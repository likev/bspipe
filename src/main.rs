#![cfg_attr(
    not(any(feature = "default-resolver", feature = "ring-accelerated",)),
    allow(dead_code, unused_extern_crates, unused_imports)
)]
//! This is a barebones TCP Client/Server that establishes a `Noise_NN` session, and sends
//! an important message across the wire.
//!
//! # Usage
//! Run the server a-like-a-so `cargo run --example simple -- -s`, then run the client
//! as `cargo run --example simple` to see the magic happen.

use lazy_static::lazy_static;

use clap::App;
use snow::{params::NoiseParams, Builder};

use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread,
};

static SECRET: &[u8] = b"i don't care for fidget spinners";
lazy_static! {
    static ref PARAMS: NoiseParams = "Noise_XXpsk3_25519_ChaChaPoly_BLAKE2s".parse().unwrap();
}

#[cfg(any(feature = "default-resolver", feature = "ring-accelerated"))]
fn main() {
    let matches = App::new("simple").args_from_usage("-s --server 'Server mode'").get_matches();

    if matches.is_present("server") {
        run_server();
    } else {
        run_client();
    }
    println!("all done.");
}

#[cfg(any(feature = "default-resolver", feature = "ring-accelerated"))]
fn run_server() {
    let mut buf = vec![0u8; 65535];

    // Initialize our responder using a builder.
    let builder: Builder<'_> = Builder::new(PARAMS.clone());
    let static_key = builder.generate_keypair().unwrap().private;
    let mut noise =
        builder.local_private_key(&static_key).psk(3, SECRET).build_responder().unwrap();

    // Wait on our client's arrival...
    println!("listening on 127.0.0.1:9999");
    let (mut stream, _) = TcpListener::bind("127.0.0.1:9999").unwrap().accept().unwrap();

    // <- e
    noise.read_message(&recv(&mut stream).unwrap(), &mut buf).unwrap();

    // -> e, ee, s, es
    let len = noise.write_message(&[0u8; 0], &mut buf).unwrap();
    send(&mut stream, &buf[..len]);

    // <- s, se
    noise.read_message(&recv(&mut stream).unwrap(), &mut buf).unwrap();

    // Transition the state machine into transport mode now that the handshake is complete.
    let mut noise = noise.into_transport_mode().unwrap();

    /*
    while let Ok(msg) = recv(&mut stream) {
        let len = noise.read_message(&msg, &mut buf).unwrap();
        println!("client said: {}", String::from_utf8_lossy(&buf[..len]));
    }
    */

    let stream_clone = stream.try_clone().expect("clone failed...");

    // Wait on our client's arrival...
    let listener_address = "127.0.0.1:8080";
    let listener = TcpListener::bind(listener_address).unwrap();
    println!("listen on address: {}", listener_address);

    let (tx0, rx) = mpsc::channel();

    for listener_stream in listener.incoming() {
        //thread::spawn(|| {//new thread every income
            println!("new listener_stream");

            let mut listener_stream = listener_stream.unwrap();
            let mut listener_stream_clone = listener_stream.try_clone().expect("clone failed...");

            let tx = tx0.clone();//for future new thread 

            thread::spawn(move || {// for read block

                let mut buffer = vec![0u8; 0];

                println!("listener_stream read to end...");

                loop{//read data loop
                    let mut buffer_inner = vec![0u8; 65535];

                    if let Ok(len) = listener_stream.read(&mut buffer_inner){//may block
                        if len>0{
                            println!("len: {} data: {}",len, String::from_utf8_lossy(&buffer_inner[..len]));

                            //buffer.append(&mut buffer_inner[..len].to_vec());
                            buffer_inner.resize(len, 0);

                            tx.send(buffer_inner).unwrap();
                        }else{
                            println!("OK: listener_stream maybe close...");
                            break;
                            //continue;
                        }
                        
                    } else{
                        println!("Error: listener_stream maybe read to end...");
                        break;
                        //continue;
                    }
                }
                
                
            });


            let received = rx.recv().unwrap();
            println!("Got: {}", received.len());
            
            println!("listener_stream write to noise...");
            let len = noise.write_message(&received, &mut buf).unwrap();
            send(&mut stream, &buf[..len]);

            println!("recv noise ...");
            if let Ok(msg) = recv(&mut stream){
                let len = noise.read_message(&msg, &mut buf).unwrap();
                println!("{}", String::from_utf8_lossy(&buf[..len]));
                
                println!("noise to listener_stream ...");
                listener_stream_clone.write_all(&buf[..len]);
            }
        
        //});//end of thread

    }
    
    

    println!("connection closed.");
}

#[cfg(any(feature = "default-resolver", feature = "ring-accelerated"))]
fn run_client() {
    let mut buf = vec![0u8; 65535];

    // Initialize our initiator using a builder.
    let builder: Builder<'_> = Builder::new(PARAMS.clone());
    let static_key = builder.generate_keypair().unwrap().private;
    let mut noise =
        builder.local_private_key(&static_key).psk(3, SECRET).build_initiator().unwrap();

    // Connect to our server, which is hopefully listening.
    let mut stream = TcpStream::connect("127.0.0.1:9999").unwrap();
    println!("connected...");

    // -> e
    let len = noise.write_message(&[], &mut buf).unwrap();
    send(&mut stream, &buf[..len]);

    // <- e, ee, s, es
    noise.read_message(&recv(&mut stream).unwrap(), &mut buf).unwrap();

    // -> s, se
    let len = noise.write_message(&[], &mut buf).unwrap();
    send(&mut stream, &buf[..len]);

    let mut noise = noise.into_transport_mode().unwrap();
    println!("session established...");

    /*
    // Get to the important business of sending secured data.
    for _ in 0..10 {
        let len = noise.write_message(b"HACK THE PLANET", &mut buf).unwrap();
        send(&mut stream, &buf[..len]);
    }
    */

    loop{
        println!("local_stream wait recv noise ...");
        
        if let Ok(msg) = recv(&mut stream){
            println!("local_stream wait recving noise ...");

            let len = noise.read_message(&msg, &mut buf).unwrap();

            println!("len: {} noise: {}",len, String::from_utf8_lossy(&buf[..len]));

            // Connect to local service
            let mut local_stream = TcpStream::connect("127.0.0.1:7878").unwrap();

            println!("local_stream write ...");
            let len = local_stream.write_all(&buf[..len]).unwrap();
            
            let mut buffer = vec![0u8; 65535];
            println!("local_stream read ...");
            let len = local_stream.read(&mut buffer).unwrap();
            println!("{}", String::from_utf8_lossy(&buffer[..len]));

            println!("local_stream pipe to noise");
            let len = noise.write_message(&buffer[..len], &mut buf).unwrap();
            send(&mut stream, &buf[..len]);
        }else{
            println!("local_stream can't  recv noise , close...");
            break;
        }
    
        
    }
    

    println!("notified server of intent to hack planet.");
}

/// Hyper-basic stream transport receiver. 16-bit BE size followed by payload.
fn recv(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut msg_len_buf = [0u8; 2];
    stream.read_exact(&mut msg_len_buf)?;
    let msg_len = ((msg_len_buf[0] as usize) << 8) + (msg_len_buf[1] as usize);

    println!("recv msg_len {}", msg_len);

    let mut msg = vec![0u8; msg_len];
    stream.read_exact(&mut msg[..])?;
    Ok(msg)
}

/// Hyper-basic stream transport sender. 16-bit BE size followed by payload.
fn send(stream: &mut TcpStream, buf: &[u8]) {
    let msg_len_buf = [(buf.len() >> 8) as u8, (buf.len() & 0xff) as u8];
    stream.write_all(&msg_len_buf).unwrap();
    stream.write_all(buf).unwrap();
}

#[cfg(not(any(feature = "default-resolver", feature = "ring-accelerated")))]
fn main() {
    panic!("Example must be compiled with some cryptographic provider.");
}
