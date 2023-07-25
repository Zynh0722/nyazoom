use headers::{self, Header, HeaderName, HeaderValue};

#[derive(Debug)]
pub struct ForwardedFor(String);

pub static FF_TEXT: &str = "x-forwarded-for";

pub static FF_NAME: HeaderName = HeaderName::from_static(FF_TEXT);

impl Header for ForwardedFor {
    fn name() -> &'static HeaderName {
        &FF_NAME
    }

    fn decode<'i, I>(values: &mut I) -> Result<Self, headers::Error>
    where
        Self: Sized,
        I: Iterator<Item = &'i headers::HeaderValue>,
    {
        let value = values
            .next()
            .ok_or_else(headers::Error::invalid)?
            .to_str()
            .map_err(|_| headers::Error::invalid())?
            .to_owned();

        Ok(ForwardedFor(value))
    }

    fn encode<E: Extend<headers::HeaderValue>>(&self, values: &mut E) {
        values.extend(std::iter::once(HeaderValue::from_str(&self.0).unwrap()));
    }
}
