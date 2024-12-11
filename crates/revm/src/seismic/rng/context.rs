type BlockHeader = [u8; 32];

#[derive(Clone, Debug, Copy)]
pub struct Context { // potentially could hold other contextual info
    pub header: BlockHeader,
}

impl Context {
    pub fn new(header: BlockHeader) -> Self {
        Self { header }
    }
    
}
