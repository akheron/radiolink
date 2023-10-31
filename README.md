# radiolink

This is a research project into embedded development.

The goal is to create a simple serial link between two [BBC micro:bit](https://microbit.org/) devices over 2.4 GHz
radio. This would enable for example a PPP connection between two locations over a distance of a few tens of metres.

micro:bit has an nRF51822 microcontroller, which features UART and a pretty low level 2.4 GHz radio interface.
Here's a diagram of the setup:
```
client <------> micro:bit ~~~~~~~ micro:bit <------> client
        serial             radio             serial
```

The first iteration used micropython (see `py/radiolink.py`). It didn't work too well, mostly due to the lack of an API
that would be low-level enough, and eventually running out of memory.

The current iteration uses Rust, but is still very much a work in progress, mostly just testing the UART as of now.

## Development

Install some helpers: `cargo install probe-rs flip-link`

Build and flash the firmware: `cargo run --release --target thumbv6m-none-eabi`

Currently uses pins 0 and 1 of the edge connector for UART RX and TX.
