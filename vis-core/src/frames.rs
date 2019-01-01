use crate::{analyzer, recorder};
use std::{time, rc, cell};


#[derive(Debug)]
pub struct Frame<R: Send> {
    pub time: f32,
    pub frame: usize,
    info: rc::Rc<cell::RefCell<triple_buffer::Output<R>>>,
}

impl<R: Send> Frame<R> {
    pub fn lock_info<F, O>(&self, f: F) -> O
    where
        F: FnOnce(&R) -> O,
    {
        f(self.info.borrow_mut().read())
    }
}

#[derive(Debug)]
pub struct Frames<R, A>
where
    R: Clone + Send + 'static,
    for<'r> A: FnMut(&'r mut R, &analyzer::SampleBuffer) -> &'r mut R + Send + 'static,
{
    info: rc::Rc<cell::RefCell<triple_buffer::Output<R>>>,
    analyzer: Option<(A, triple_buffer::Input<R>)>,
    recorder: Box<dyn recorder::Recorder>,
}

impl<R, A> Frames<R, A>
where
    R: Clone + Send + 'static,
    for<'r> A: FnMut(&'r mut R, &analyzer::SampleBuffer) -> &'r mut R + Send + 'static,
{
    pub fn from_vis(vis: crate::Visualizer<R, A>) -> Frames<R, A> {
        let (inp, outp) = triple_buffer::TripleBuffer::new(vis.initial).split();
        let mut f = Frames {
            info: rc::Rc::new(cell::RefCell::new(outp)),
            analyzer: Some((vis.analyzer, inp)),
            recorder: vis.recorder.unwrap_or_else(|| recorder::default()),
        };

        if vis.async_analyzer.unwrap_or(false) {
            f.detach_analyzer();
        }

        f
    }

    pub fn detach_analyzer(&mut self) {
        let (mut analyzer, mut info) = self.analyzer.take().unwrap();
        let buffer = self.recorder.sample_buffer().clone();

        std::thread::Builder::new()
            .name("analyzer".into())
            .spawn(move || {
                loop {
                    analyzer(info.raw_input_buffer(), &buffer);
                    info.raw_publish();
                    // Todo, properly implement this detacher
                    std::thread::sleep_ms(1);
                }
            }).unwrap();
    }

    pub fn iter<'a>(&'a mut self) -> FramesIter<'a, R, A> {
        FramesIter {
            buffer: self.recorder.sample_buffer().clone(),
            visualizer: self,
            start_time: time::Instant::now(),
            frame: 0,
        }
    }
}

#[derive(Debug)]
pub struct FramesIter<'a, R, A>
where
    R: Clone + Send + 'static,
    for<'r> A: FnMut(&'r mut R, &analyzer::SampleBuffer) -> &'r mut R + Send + 'static,
{
    visualizer: &'a mut Frames<R, A>,
    buffer: analyzer::SampleBuffer,
    start_time: time::Instant,
    frame: usize,
}

impl<'a, R, A> Iterator for FramesIter<'a, R, A>
where
    R: Clone + Send + 'static,
    for<'r> A: FnMut(&'r mut R, &analyzer::SampleBuffer) -> &'r mut R + Send + 'static,
{
    type Item = Frame<R>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some((ref mut analyzer, ref mut info)) = self.visualizer.analyzer {
            analyzer(info.raw_input_buffer(), &self.buffer);
            info.raw_publish();
        }

        let frame = self.frame;
        self.frame += 1;

        Some(Frame {
            time: crate::helpers::time(self.start_time),
            frame,
            info: self.visualizer.info.clone(),
        })
    }
}
