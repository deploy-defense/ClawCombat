set PKG_CONFIG_PATH=C:\vcpkg\installed\x64-windows\lib\pkgconfig
set PKG_CONFIG=C:\vcpkg\installed\x64-windows\tools\pkgconf\pkgconf.exe
set PATH=C:\vcpkg\installed\x64-windows\bin;%PATH%
cargo run --bin battle_server -- Demo1 --rep-address tcp://0.0.0.0:4255 --bind-address tcp://0.0.0.0:4256