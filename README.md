# retrofetch

[![CI](https://github.com/roblillack/retrofetch/actions/workflows/ci.yml/badge.svg)](https://github.com/roblillack/retrofetch/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/retrofetch.svg)](https://crates.io/crates/retrofetch)
[![license](https://img.shields.io/crates/l/retrofetch.svg)](LICENSE)

A retro _"About This Computer"_ system info screen for your desktop — like
`neofetch`, but as a little classic-themed window instead of ASCII art.

<p align="center">
  <img src="https://raw.githubusercontent.com/roblillack/retrofetch/main/screenshot.png" width="445" alt="retrofetch showing an About This Computer window">
</p>

retrofetch pops open a small _Windows 3.1_–styled dialog (rendered with
[saudade](https://crates.io/crates/saudade)) that shows your machine name, OS,
hardware and uptime, with the logo of your operating system or Linux
distribution. It runs on Linux, the BSDs, macOS and Windows.

## Install

```sh
cargo install retrofetch
```

## Run

```sh
retrofetch
```

## License

[MIT](LICENSE)
