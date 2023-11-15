const XON: u8 = 0x11;
const XOFF: u8 = 0x13;

const QUEUE_SIZE: usize = 1024;

pub struct Queue {
    queue: heapless::spsc::Queue<u8, QUEUE_SIZE>,

    /// Flow control requested from the outside
    control: Option<u8>,

    /// Have we requested XOFF?
    xoff_on: bool,
}

impl Queue {
    pub fn new() -> Self {
        Self {
            queue: heapless::spsc::Queue::new(),
            control: None,
            xoff_on: false,
        }
    }

    pub fn enqueue(&mut self, byte: u8) {
        self.queue.enqueue(byte).unwrap();
    }

    pub fn dequeue(&mut self) -> Option<u8> {
        match self.control {
            Some(c) => {
                self.control = None;
                Some(c)
            }
            None => self.queue.dequeue(),
        }
    }

    pub fn len(&self) -> usize {
        (match self.control {
            Some(_) => 1,
            None => 0,
        }) + self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Request flow control by sending XON/XOFF to the target queue if needed
    pub fn flow_control(&mut self, target: &mut Queue) {
        if self.queue.len() > QUEUE_SIZE / 2 && !self.xoff_on {
            self.xoff_on = true;
            target.control = Some(XOFF);
        } else if self.xoff_on && self.queue.len() < QUEUE_SIZE / 4 {
            self.xoff_on = false;
            target.control = Some(XON);
        }
    }
}
