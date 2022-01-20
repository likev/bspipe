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

use clap::{arg, app_from_crate};

use snow::{params::NoiseParams, Builder, TransportState};

use std::{
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    thread,
    time::Duration,

    sync::{Arc, Mutex},
    mem::drop,

    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},

    collections::HashMap,
};

//static SECRET: &[u8] = b"i don't care for fidget spinners";
lazy_static! {
    static ref PARAMS: NoiseParams = "Noise_XXpsk3_25519_ChaChaPoly_BLAKE2s".parse().unwrap();
}

#[cfg(any(feature = "default-resolver", feature = "ring-accelerated"))]
fn main() {
    //let matches = App::new("simple").args_from_usage("-s --server 'Server mode'").get_matches();
    let matches = 
        app_from_crate!()
        .about("bspipe A Rust implementation of Bidirectional Secure Pipe")
        .args(&[
            arg!(-l  --listen <host> "listen address to accept connection, must be identical to remote")
                .default_missing_value("127.0.0.1:9999").default_value("127.0.0.1:9999").required(false),
            
            arg!(-a  --agent <host> "agent address others connect to")
                .default_missing_value("127.0.0.1:8080").default_value("127.0.0.1:8080").required(false),
            
            arg!(-s  --service <host> "original service address")
                .default_missing_value("127.0.0.1:7878").required(false),
            
            arg!(-r  --remote <host> "remote address to connect to, must be identical to listen")
                .default_missing_value("127.0.0.1:9999").default_value("127.0.0.1:9999").required(false),
            
            arg!(-t  --token <string> "Secure Token")
                .default_value("i don't care for fidget spinners").required(false),
        ])
        .after_help("Longer explanation to appear after the options when 
                        displaying the help information from --help or -h")
        .get_matches();

    //println!("matches:\n{:#?}", matches);

    println!("matches service: {}", matches.is_present("listen"));

    if !matches.is_present("service") {
        println!("run_proxy_service_end");
        run_proxy_service_end(matches);
    } else {
        println!("run_original_service_end");
        run_original_service_end(matches);
    }
    println!("all done.");
}

fn string_resize(s: & str, size: usize, value: u8)->Vec<u8> {
    let mut m = s.as_bytes().to_vec();
    m.resize(size, value);

    return m;
}

fn create_listen_stream(matches: &clap::ArgMatches)->(TcpStream, TransportState) {
    let mut buf = vec![0u8; 65535];

    // Initialize our responder using a builder.
    let builder: Builder<'_> = Builder::new(PARAMS.clone());
    let static_key = builder.generate_keypair().unwrap().private;

    let token = matches.value_of("token").unwrap();
    let mut noise =
            builder
                .local_private_key(&static_key)
                .psk(3, &string_resize(token, 32, 0))
                .build_responder().unwrap();

    // Wait on our client's arrival...
    let address = matches.value_of("listen").unwrap();
    println!("listening on {}", address);
    let (mut stream, _) = TcpListener::bind(address).unwrap().accept().unwrap();

    // <- e
    noise.read_message(&recv(&mut stream).unwrap(), &mut buf).unwrap();

    // -> e, ee, s, es
    let len = noise.write_message(&[0u8; 0], &mut buf).unwrap();
    send(&mut stream, &buf[..len]);

    // <- s, se
    noise.read_message(&recv(&mut stream).unwrap(), &mut buf).unwrap();

    // Transition the state machine into transport mode now that the handshake is complete.
    let noise = noise.into_transport_mode().unwrap();

    return (stream, noise);
}

#[cfg(any(feature = "default-resolver", feature = "ring-accelerated"))]
fn run_proxy_service_end(matches: clap::ArgMatches) {

    let (mut stream, noise) = 
        if matches.occurrences_of("listen") > 0 {
            create_listen_stream(&matches)
        }else{
            create_connect_stream(&matches)
        };

    let arc_noise = Arc::new(Mutex::new(noise));

    // Wait on our client's arrival...
    let listener_address = matches.value_of("agent").unwrap();

    let listener = TcpListener::bind(listener_address).unwrap();
    println!("agent listen on address: {}", listener_address);

    let (tx0, rx) = mpsc::channel();

    let handles = Arc::new(Mutex::new(vec![]));

    let handles1 = Arc::clone(&handles);

    let mut stream_clone = stream.try_clone().expect("clone failed...");
            
    let arc_noise1 = Arc::clone(&arc_noise);

    thread::spawn(move ||{
        let mut buf = vec![0u8; 65535];

        loop{             

            let received:Vec<u8> = rx.recv().unwrap();
            println!("Got: {}", received.len());

            println!("listener_stream wait arc_noise1.lock()...");
            let mut noise = arc_noise1.lock().unwrap();
            println!("listener_stream get arc_noise1.lock()...");
            
            println!("listener_stream write to noise...");
            let len = noise.write_message(&received, &mut buf).unwrap();
            drop(noise);

            println!("listener_stream send noise...");
            send(&mut stream, &buf[..len]);
            println!("listener_stream sended noise...");

            thread::sleep(Duration::from_millis(1));//for system thread change
        }
    });

    println!("recv noise ...");
    
    let arc_noise2 = Arc::clone(&arc_noise);
    thread::spawn(move ||{

        read_and_send_back("listener-stream", &mut stream_clone, arc_noise2, handles);

    });

    let mut counter = 0i32;

    for listener_stream in listener.incoming() {
    //thread::spawn(|| {//new thread every income
        println!("new listener_stream");

        let mut listener_stream = listener_stream.unwrap();//for read
        let listener_stream_clone = listener_stream.try_clone().expect("clone failed...");//for later write

        println!("listener_stream wait handles1.lock...");
        let mut vec = handles1.lock().unwrap();
        counter = counter + 1;

        let peer_addr = listener_stream_clone.peer_addr().unwrap();

        let mut hasher = DefaultHasher::new();
        peer_addr.hash(&mut hasher);

        vec.push((hasher.finish(), listener_stream_clone));

        drop(vec);


        let tx = tx0.clone();//for future new thread 

        thread::spawn(move ||{

            read_and_send("listener-stream", &mut listener_stream, tx);
        });
    }

    println!("connection closed.");
}

fn read_and_send_back(name: &str, stream: &mut TcpStream, noise: Arc<Mutex<TransportState>>, handles:Arc<Mutex<Vec<(u64, TcpStream)>>>){// for read block

    let mut buf = vec![0u8; 65535];

    loop{
        println!("{} wait recv and send back...", name);
        if let Ok(msg) = recv(stream){

            println!("{} wait arc_noise2.lock()...", name);
            let mut noise = noise.lock().unwrap();
            println!("{} get arc_noise2.lock()...", name);

            println!("before noise read secret message: {}", String::from_utf8_lossy(&msg));

            let len = noise.read_message(&msg, &mut buf).unwrap();
            
            drop(noise);

            let peer_hash = u64::from_be_bytes(buf[..8].try_into().expect("slice with incorrect length"));
            println!("peer_hash {} ", peer_hash);

            println!("{} wait handles.lock...", name);
            let vec = handles.lock().unwrap();

            for item in vec.iter(){
                if item.0 == peer_hash{
                    let mut stream_back = &item.1;

                    let buffer_data = &buf[8..len];
                    
                    println!("{}", String::from_utf8_lossy(&buffer_data));
                    
                    println!("noise to listener_stream ...");
                    stream_back.write_all(&buffer_data).unwrap();
                }
            }
           
            drop(vec);
        }else{
            //todo when stream close
            break;
        }
        thread::sleep(Duration::from_millis(1));//for system thread change
    }
}

fn read_and_send(name: &str, stream: &mut TcpStream, tx: mpsc::Sender<Vec<u8>>){// for read block

    //let mut buffer = vec![0u8; 0];

    let peer_addr = stream.peer_addr().unwrap();

    let mut hasher = DefaultHasher::new();
    peer_addr.hash(&mut hasher);

    println!("listen_peer hasher: {}", hasher.finish());

    println!("{} read to end...", name);
    
    loop{//read data loop

        let mut buffer_inner = vec![0u8; 0];//65535

        buffer_inner.append(&mut hasher.finish().to_be_bytes().to_vec());//8
        buffer_inner.resize(65535, 0);
        
        if let Ok(len) = stream.read(&mut buffer_inner[8..]){//may block
            if len>0{
                let totle_size = 8 + len;
                println!("len: {} data: \n\n{}\n\n",len, String::from_utf8_lossy(&buffer_inner[8..totle_size]));

                //buffer.append(&mut buffer_inner[..len].to_vec());
                buffer_inner.resize(totle_size, 0);

                tx.send(buffer_inner).unwrap();

                thread::sleep(Duration::from_millis(1));//for system thread change
            }else{
                println!("OK: {} maybe close...", name);
                break;
                //continue;
            }
            
        } else{
            println!("Error: {} maybe read to end...", name);
            break;
            //continue;
        }
    }
}

#[cfg(any(feature = "default-resolver", feature = "ring-accelerated"))]

fn connect_loop(address: &str)->TcpStream{
    let stream = loop {
        println!("try connecte to {}", address);
        if let Ok(stream) = TcpStream::connect(address){
            break stream;
        }else{
            println!("connected fail wait 5 secs...");
            thread::sleep(Duration::from_secs(5));
            println!("connected retry...");
            continue;
        }
    };
    println!("local addr {:#?} \n peer addr {:#?}", stream.local_addr().unwrap(), stream.peer_addr().unwrap());

    return stream;
}

fn create_connect_stream(matches: &clap::ArgMatches)->(TcpStream, TransportState) {
    let mut buf = vec![0u8; 65535];

    // Initialize our initiator using a builder.
    let builder: Builder<'_> = Builder::new(PARAMS.clone());
    let static_key = builder.generate_keypair().unwrap().private;

    let token = matches.value_of("token").unwrap();
    
    let mut noise =
                builder
                    .local_private_key(&static_key)
                    .psk(3, &string_resize(token, 32, 0))
                    .build_initiator().unwrap();

    // Connect to our server, which is hopefully listening.
    
    let mut stream = connect_loop(matches.value_of("remote").unwrap());

    println!("connected...");

    // -> e
    let len = noise.write_message(&[], &mut buf).unwrap();
    send(&mut stream, &buf[..len]);

    // <- e, ee, s, es
    noise.read_message(&recv(&mut stream).unwrap(), &mut buf).unwrap();

    // -> s, se
    let len = noise.write_message(&[], &mut buf).unwrap();
    send(&mut stream, &buf[..len]);

    let noise = noise.into_transport_mode().unwrap();
    println!("session established...");

    return (stream, noise);
}

fn run_original_service_end(matches: clap::ArgMatches) {
    let (mut stream, noise) = 
        if matches.occurrences_of("remote") > 0 {
            create_connect_stream(&matches)
        }else{
            create_listen_stream(&matches)
        };

    let arc_noise = Arc::new(Mutex::new(noise));

    let mut handles = vec![];
    let new_peer_data = Arc::new(Mutex::new(vec![(0u64, vec![0u8;0]);0]));
    let using_peers = Arc::new(Mutex::new(HashMap::new()));//vec![0u64;0]

    let service_addr:String = matches.value_of("service").unwrap().to_string();
    

    for thread_index in 0..10 {//ten threads to connect to service
        let new_peer_data = Arc::clone(&new_peer_data);

        let using_peers = Arc::clone(&using_peers);
        let using_peers2 = Arc::clone(&using_peers);

        let arc_noise = Arc::clone(&arc_noise);
        let mut stream_clone = stream.try_clone().expect("clone failed...");

        let service_addr_clone = service_addr.clone();

        let handle = thread::spawn(move||{// 

            let service_addr_clone2 = service_addr_clone.clone();
            
            let local_stream = connect_loop(&service_addr_clone);
            let local_stream_read = local_stream.try_clone().expect("clone failed...");//for later read
            
            let arc_local_stream = Arc::new(Mutex::new(local_stream));
            let arc_local_stream_clone = Arc::clone(&arc_local_stream);

            let arc_local_stream_read = Arc::new(Mutex::new(local_stream_read));

            let peer_hash = Arc::new(Mutex::new(0u64));
            let peer_hash_clone = Arc::clone(&peer_hash);

            thread::spawn(move||{//read thread 
                loop{

                    let peer_hash = peer_hash_clone.lock().unwrap();

                    if *peer_hash == 0 {
                        drop(peer_hash);

                        thread::sleep(Duration::from_millis(1000));//for system thread change
                        continue;
                    }
                    drop(peer_hash);


                    let mut buffer = vec![0u8; 65535];

                    //buffer_inner.append(&mut peer_hash.to_be_bytes().to_vec());//8
                    //buffer_inner.resize(65535, 0);

                    let mut local_stream_read = arc_local_stream_read.lock().unwrap();

                    println!("local_stream read ...");
                    let mut read_success = false;
                    let mut len_read = 0;
                    if let Ok(len) = local_stream_read.read(&mut buffer){
                        if len > 0{
                            len_read = len;
                            read_success = true;
                            buffer.resize(len_read, 0);

                            println!("len: {} read from service: {}",len_read, String::from_utf8_lossy(&buffer));
                        }else{
                            println!("local_stream read over...");
                            //connect close
                        }
                    }else{
                        println!("local_stream read lost...");
                        //connect lost
                    }

                    
                    let mut peer_hash = peer_hash_clone.lock().unwrap();
                    if !read_success{
                        // re_connect
                        // using_peers remove peer_hash
                        // in_use = false
                        
                        let mut using_peers = using_peers2.lock().unwrap();
                        using_peers.remove(&(*peer_hash));
                        drop(using_peers);

                        *peer_hash = 0;


                        println!("local_stream close or lost, retry ...");

                        let mut local_stream_clone = arc_local_stream_clone.lock().unwrap();

                        *local_stream_read = connect_loop(&service_addr_clone2);
                        *local_stream_clone = local_stream_read.try_clone().expect("clone failed...");

                        drop(local_stream_clone);

                    }

                    println!("local_stream read len {} ...", len_read);

                    drop(local_stream_read);

                    let total_len = 8 + len_read;

                    
                    let mut buffer_inner = peer_hash.to_be_bytes().to_vec();
                    drop(peer_hash);

                    buffer_inner.append(&mut buffer);
                    println!("{}", String::from_utf8_lossy(&buffer_inner[8..total_len]));
                    buffer_inner.resize(total_len, 0);
    
                    let mut arc_noise = arc_noise.lock().unwrap();
                    println!("local_stream pipe to noise");

                    let mut buf = vec![0u8; 65535];//65535
                    let len = arc_noise.write_message(&buffer_inner, &mut buf).unwrap();

                    send(&mut stream_clone, &buf[..len]);

                    drop(arc_noise);
                }
                
            });

            loop{             
                let mut new_peer_data = new_peer_data.lock().unwrap();

                let mut index = 0;
                loop{//each new piece data
                    if index >= new_peer_data.len(){
                        break;
                    }

                    let received = &new_peer_data[index];

                    println!("local_stream check received {:?}", received);

                    let received_hash = received.0;
                    println!("local_stream {} check received new data hash {}", thread_index, received_hash);

                    let mut using_peers = using_peers.lock().unwrap();

                    println!("local_stream using_peers {:#?}", using_peers);

                    let mut peer_hash = peer_hash.lock().unwrap();

                    println!("local_stream peer_hash {:#?}", peer_hash);

                    let mut self_work = false;

                    if *peer_hash != 0 { // in use
                        if *peer_hash == received_hash {
                            self_work = true;

                        }else{//let other do work
                            //continue;
                        }
                    }else{
                        if using_peers.get(&received_hash) != None {//let other do work
                            //continue;
                        }else{
                            self_work = true;
                            
                            *peer_hash = received_hash;
                            using_peers.insert(received_hash, true);
                        }
                    }

                    if !self_work {
                        index += 1;
                        continue;
                    }

                    
                    drop(peer_hash);
                    

                    drop(using_peers);

                    let buffer_data = new_peer_data.swap_remove(index).1;

                    println!("local_stream will write {}...", String::from_utf8_lossy(&buffer_data));

                    let mut local_stream = arc_local_stream.lock().unwrap();
                    println!("local_stream write ...");
                    local_stream.write_all(&buffer_data).unwrap();
                    local_stream.flush().unwrap();
                    drop(local_stream);

                    index += 1;

                }
                drop(new_peer_data);

                thread::sleep(Duration::from_millis(1000));//for system thread change
                
            }

        });
        
        handles.push(handle);

    }

    let mut buf = vec![0u8; 65535];
    loop{
        println!("local_stream wait recv noise ...");
        
        if let Ok(msg) = recv(&mut stream){
            println!("local_stream wait recving noise ...");

            let mut arc_noise = arc_noise.lock().unwrap();
            let len = arc_noise.read_message(&msg, &mut buf).unwrap();
            drop(arc_noise);

            let peer_hash = u64::from_be_bytes(buf[..8].try_into().expect("slice with incorrect length"));
            println!("peer_hash {} ", peer_hash);

            let buffer_data = &buf[8..len];

            println!("len: {} noise: {}",len-8, String::from_utf8_lossy(buffer_data));

            let mut new_peer_data = new_peer_data.lock().unwrap();
            new_peer_data.push((peer_hash, buffer_data.to_vec()));

            drop(new_peer_data);

            
        }else{
            println!("local_stream can't  recv noise , close...");
            break;
        }
    
        thread::sleep(Duration::from_millis(1));//for system thread change
        
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
