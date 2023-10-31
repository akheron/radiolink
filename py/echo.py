import micropython
from microbit import display, uart, sleep, pin0, pin1

MAX_PACKET_LEN = 64
BAUDRATE = 115200
RX = pin0
TX = pin1

def on(x, y):
    display.set_pixel(x, y, 9)


def toggle(x, y):
    value = display.get_pixel(x, y)
    display.set_pixel(x, y, 0 if value else 9)


def off():
    display.clear()

def main():
    # Disable raising KeyboardInterrupt on Ctrl-C (ASCII 0x3)
    micropython.kbd_intr(-1)

    uart.init(baudrate=BAUDRATE, rx=RX, tx=TX)

    on(0, 0)
    on(4, 4)
    sleep(1000)
    off()

    while True:
        if uart.any():
            tx = uart.read(MAX_PACKET_LEN)
            uart.write(tx)

main()
