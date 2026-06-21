set PKG_CONFIG_PATH=C:\vcpkg\installed\x64-windows\lib\pkgconfig
set PKG_CONFIG=C:\vcpkg\installed\x64-windows\tools\pkgconf\pkgconf.exe
set PATH=C:\Windows\System32;C:\vcpkg\installed\x64-windows\bin;%PATH%
set RUST_BACKTRACE=1
cargo run --release --bin battle_gui -- Demo2 assets/demo2_deployment.json --embedded-server --server-rep-address=tcp://127.0.0.1:4255 --server-bind-address=tcp://127.0.0.1:4256 --side=a --side-a-control=W --side-a-control=NW --side-a-control=SW --side-b-control=ALL --init-sync	