import micropython
from microbit import display, uart, sleep, pin0, pin1
import radio
import utime

BAUDRATE = 115200
RX = pin0
TX = pin1
MAX_PACKET_LEN = 160
MAX_DATA_LEN = MAX_PACKET_LEN - 3

radio_config = {
    'length': MAX_PACKET_LEN,
    'queue': 16,  # incoming queue size
    'channel': 55,
    'power': 7,
    'address': 0x12E5AB5C,  # random
    'group': 0,
    'data_rate': radio.RATE_1MBIT,
}


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

    radio.config(**radio_config)
    radio.on()

    on(2, 2)
    sleep(1000)
    off()

    pending_tx = []
    pending_rx = []
    next_tx_packet_id = 0
    waiting_for_ack = None
    waited_for_ack = 0
    last_rx_acked_id = None

    while True:
        if uart.any():
            # start = utime.ticks_us()
            tx = uart.read(MAX_DATA_LEN)
            # end = utime.ticks_us()
            # print("read %d bytes from uart (took %d us)" % (len(tx), utime.ticks_diff(end, start)))
            pending_tx.append(tx)
            # print("tx: %d entries, %d bytes, %d avg bytes" % (len(pending_tx), sum(len(x) for x in pending_tx), sum(len(x) for x in pending_tx) / len(pending_tx)))

        rx = radio.receive_bytes()
        if rx is not None:
            # parse packet
            packet_type = rx[0]
            packet_id = rx[1]
            packet_data = rx[2:]

            if packet_type == ord('d'):
                # write to uart if not a duplicate
                if packet_id != last_rx_acked_id:
                    pending_rx.append(packet_data)

                # send ack
                ack = bytes([ord('a'), packet_id])
                radio.send_bytes(ack)
                last_rx_acked_id = packet_id

            elif packet_type == ord('a'):
                # ack received
                if waiting_for_ack is not None and packet_id == waiting_for_ack[0]:
                    # print("ack received for packet %d" % packet_id)
                    waiting_for_ack = None
        elif pending_rx:
            uart.write(pending_rx.pop(0))

        if waiting_for_ack is None and pending_tx:
            tx = pending_tx.pop(0)
            packet_id = next_tx_packet_id
            next_tx_packet_id = (next_tx_packet_id + 1) % 256
            # start = utime.ticks_us()
            radio.send_bytes(bytes([ord('d'), packet_id]) + tx)
            # end = utime.ticks_us()
            waiting_for_ack = (packet_id, tx)
            waited_for_ack = 0
            # print("sent packet %d of %d bytes (took %d us)" % (packet_id, len(tx), utime.ticks_diff(end, start)))

        if waiting_for_ack is not None:
            waited_for_ack += 1
            if waited_for_ack >= 1000:
                # give up
                waiting_for_ack = None
            elif waited_for_ack % 100 == 0:
                # resend
                (packet_id, tx) = waiting_for_ack
                radio.send_bytes(bytes([ord('d'), packet_id]) + tx)

main()
