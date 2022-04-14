pub mod message {
    use h264_nal_paging::H264NalUnit;

    #[derive(Clone, Debug)]
    pub enum Message {
        NalUnit(H264NalUnit),
        Abort,
    }
}
