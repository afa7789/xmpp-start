Name:           xmpp-start
Version:        0.1.0
Release:        1%{?dist}
Summary:        Native XMPP desktop messenger

License:        MIT
URL:            https://github.com/owner/xmpp-start

BuildRequires:  rust, cargo, pkgconfig(openssl), pkgconfig(dbus-1)
Requires:       openssl-libs, dbus-libs

%description
A native XMPP desktop messenger built with Rust and iced.

%build
cargo build --release --bin xmpp-start

%install
install -Dm755 target/release/xmpp-start %{buildroot}%{_bindir}/xmpp-start

%files
%{_bindir}/xmpp-start
%license LICENSE
%doc README.md
