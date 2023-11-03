# radiolink

This is a research project into embedded development.

The goal is to create a simple serial link between two [BBC micro:bit v1](https://microbit.org/) devices over 2.4 GHz
radio. This would enable for example a PPP connection between two locations over a distance of a few tens of metres.

micro:bit v1 has an nRF51822 microcontroller, which features UART and a 2.4 GHz radio with a pretty low-level API.
Here's a diagram of the setup:
```
client <------> micro:bit ~~~~~~~ micro:bit <------> client
        serial             radio             serial
```

The first iteration used micropython (see `py/radiolink.py`). It didn't work too well, mostly due to the lack of an API
that would be low-level enough, and eventually running out of memory.

The current iteration uses Rust, but is still very much a work in progress, mostly just testing the UART as of now.

## Development

Install prerequisites

```
rustup target add thumbv6m-none-eabi
cargo install flip-link
cargo install probe-rs --features cli
```

Build and flash the firmware:

```
cargo run
```

To receive debug logging from the device, set the `DEFMT_LOG` environment variable:

```
DEFMT_LOG=debug cargo run
```

Currently uses pins 0 and 1 of the edge connector for UART RX and TX.
