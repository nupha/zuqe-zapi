use crate::ffi::ZqRawValue;

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZqStatus {
    Ok = 0,
    TypeError = 1,
    RangeError = 2,
    Exception = 3,
    Error = 4,
    Pending = 5,
    InvalidArg = 6,
    GenericFailure = 7,
    PendingException = 8,
}

impl ZqStatus {
    pub fn is_ok(self) -> bool {
        matches!(self, ZqStatus::Ok)
    }
    pub fn is_err(self) -> bool {
        !self.is_ok()
    }
}

#[derive(Debug, Clone)]
pub struct ZqError {
    status: ZqStatus,
    msg: Option<String>,
    exception: Option<ZqRawValue>,
}

impl ZqError {
    pub fn new(status: ZqStatus, msg: Option<String>) -> Self {
        Self {
            status,
            msg,
            exception: None,
        }
    }

    pub fn with_exception(status: ZqStatus, exception: ZqRawValue) -> Self {
        Self {
            status,
            msg: None,
            exception: Some(exception),
        }
    }

    pub fn range_error(msg: &str) -> Self {
        Self::new(ZqStatus::RangeError, Some(msg.to_string()))
    }

    pub fn type_error(msg: &str) -> Self {
        Self::new(ZqStatus::TypeError, Some(msg.to_string()))
    }

    pub fn status(&self) -> ZqStatus {
        self.status
    }
    pub fn message(&self) -> Option<&str> {
        self.msg.as_deref()
    }
    pub fn exception(&self) -> Option<ZqRawValue> {
        self.exception
    }

    pub fn in_argument(self, name: &str) -> Self {
        Self::new(
            self.status,
            Some(format!(
                "argument `{name}`: {}",
                self.msg.unwrap_or_default()
            )),
        )
    }
}

impl From<ZqStatus> for ZqError {
    fn from(status: ZqStatus) -> Self {
        Self::new(status, None)
    }
}

impl std::fmt::Display for ZqError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.status {
            ZqStatus::Ok => write!(f, "Ok"),
            ZqStatus::TypeError => write!(f, "TypeError"),
            ZqStatus::RangeError => write!(f, "RangeError"),
            ZqStatus::Exception => write!(f, "Exception"),
            ZqStatus::Error => write!(f, "Error"),
            ZqStatus::Pending => write!(f, "Pending"),
            _ => write!(f, "Error({:?})", self.status),
        }?;
        if let Some(msg) = &self.msg {
            write!(f, ": {}", msg)?;
        }
        Ok(())
    }
}

impl std::error::Error for ZqError {}
