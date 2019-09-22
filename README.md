# InTray

![](https://api.travis-ci.org/Gowee/intray.svg?branch=master)

InTray 📥 is a lightweight and simplistic CLI tool with a clean Web UI to facilitate collecting files over HTTP.

Written in Rust and powered by the succinct web framework tide, InTray has reasonable performance and decent concurrency support. It provides nothing other than receiving upload, but with great convenience and reliability.

It can be an alternative to `python -m pyftpdlib`, even though it does not cover all usage scenarios of the latter.

## Install
~~`cargo install intray`~~

(Not published yet because some upstream denpendency are not published.)

Visit https://github.com/Gowee/intray/releases .

## Interface

### Server
```
intray [OPTIONS] [PORT]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -d, --dir <dir>            Directory to store received files [default: ./]
    -a, --ip-addr <ip_addr>    IP address to bind on [default: ::]

ARGS:
    <PORT>    Port to bind on [default: 8080]
```

### Upload with curl

**Upload without name**

`curl -X post --data-binary @FILENAME http://HOSTNAME:PORT/upload/full`

**Upload with name**

`curl -X post --data-binary @FILENAME http://HOSTNAME:PORT/upload/full/filename`

### Web UI
![A screenshot of Web UI](Screenshot.png)

## Notes
By default, intray listens on `[::]:8080`.

For Linux-based system with `net.ipv6.bindv6only` be `0` (false), which is the default, it can be reached over either IPv4
(e.g. `http://127.0.0.1:8080`) or IPv6 (e.g. `http://[::1]:8080`). See also [`IPV6(7)`](http://man7.org/linux/man-pages/man7/ipv6.7.html).

For Windows and Linux other than the above, it can only be reached over IPv6. In case IPv4 is preferred, specify
address manually.

## TODO
- [x] Rewrite with tide (deprecating the original actix-web version).
- [x] Rewrite Web UI (#1).
- [ ] Authentication.
- [ ] Support HTTPS without the need to setup Web server separatedly.
- [ ] Support to limit space usage.
- [ ] Upgrade tokio{,-fs} to 0.2 (still in alpha).
- [ ] Fallback Web-page to support IE11 and so on.
- [ ] Support retry failed task and pause/resume progressing tasks.
