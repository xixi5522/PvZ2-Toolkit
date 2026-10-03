use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use std::io::{self, Write};

pub struct XmlWriter<W: Write> {
    writer: Writer<W>,
    element_stack: Vec<String>,
}

impl<W: Write> XmlWriter<W> {
    pub fn new(writer: W) -> Self {
        let x = Writer::new_with_indent(writer, b'\t', 1);
        Self {
            writer: x,
            element_stack: Vec::new(),
        }
    }

    pub fn write_header(&mut self) -> io::Result<()> {
        self.writer
            .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
            .map_err(io::Error::other)?;
        Ok(())
    }

    pub fn start_element(&mut self, name: &str, attrs: &[(&str, &str)]) -> io::Result<()> {
        let mut elem = BytesStart::new(name);
        for &(k, v) in attrs {
            elem.push_attribute((k, v));
        }
        self.writer
            .write_event(Event::Start(elem))
            .map_err(io::Error::other)?;
        self.element_stack.push(name.to_string());
        Ok(())
    }

    pub fn end_element(&mut self, name: &str) -> io::Result<()> {
        if let Some(expected) = self.element_stack.pop()
            && expected != name
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Mismatched xml tags: expected {expected}, got {name}"),
            ));
        }
        self.writer
            .write_event(Event::End(BytesEnd::new(name)))
            .map_err(io::Error::other)?;
        Ok(())
    }

    pub fn write_element(
        &mut self,
        name: &str,
        attrs: &[(&str, &str)],
        content: Option<&str>,
    ) -> io::Result<()> {
        let mut elem = BytesStart::new(name);
        for &(k, v) in attrs {
            elem.push_attribute((k, v));
        }

        if let Some(text) = content {
            self.writer
                .write_event(Event::Start(elem.clone()))
                .map_err(io::Error::other)?;
            self.writer
                .write_event(Event::Text(BytesText::new(text)))
                .map_err(io::Error::other)?;
            self.writer
                .write_event(Event::End(elem.to_end()))
                .map_err(io::Error::other)?;
        } else {
            self.writer
                .write_event(Event::Empty(elem))
                .map_err(io::Error::other)?;
        }
        Ok(())
    }
}
