### bspipe A Rust implementation of Bidirectional Secure Pipe
---

```
USAGE:
    bspipe [OPTIONS]

OPTIONS:
    -a, --agent <host>      agent address others connect to [default: 127.0.0.1:8080]
    -h, --help              Print help information
    -l, --listen <host>     listen address to accept connection, must be identical to remote
                            [default: 127.0.0.1:9999]
    -r, --remote <host>     remote address to connect to, must be identical to listen [default:
                            127.0.0.1:9999]
    -s, --service <host>    original service address
    -t, --token <string>    Secure Token [default: "i don't care for fidget spinners"]
    -V, --version           Print version information


        Example Usage

        1. You have a remote server SSH, Access it on local

        on server: ./bspipe -s 127.0.0.1:22 -l serverIP:port -t your_token
        on local : ./bspipe -a 127.0.0.1:33 -r serverIP:port -t your_token

        now on local: ssh -oPort=33 root@127.0.0.1


        2. Your windows PC is at Home, Access it from outside

        on  Home : ./bspipe -s 127.0.0.1:3389 -r serverIP:port -t your_token
        on serve : ./bspipe -a  serverIP:8933 -l serverIP:port -t your_token

        Now you can connect to serverIP:8933
```
