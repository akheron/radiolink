# radiolink

This is a research project into embedded development in Rust.

The goal is to create a simple serial link between two [BBC micro:bit v1](https://microbit.org/) devices over 2.4 GHz
radio. This would enable for example a PPP connection between two locations over a distance of a few tens of metres.

micro:bit v1 has an nRF51822 microcontroller, which features UART and a 2.4 GHz radio with a pretty low-level API.
Here's a diagram of the setup:
```
ppp client <------> micro:bit ~~~~~~~ micro:bit <------> ppp client
            serial             radio             serial
```

## Status

- The serial over radio link is working, and `pppd` can be used to establish a connection over the link.
- Currently uses 38400 baud rate. Higher baud rates may be possible, but there's a limit to how fast the firmware can
  read and write data to the UART peripheral.
- Sends XON/XOFF flow control commands to avoid overflowing buffers in the receiving side, so XON/XOFF has to be
  configured in `pppd`.

Here's an example `pppd` command. The same commmand can be used on both ends of the link, just swap the IP addresses.
`/dev/DEVICE` is the serial device connected to the micro:bit.
```
$ pppd local nodetach noauth nolock noccp xonxoff asyncmap a0000 LOCAL-IP:REMOTE-IP /dev/DEVICE 38400
```

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
