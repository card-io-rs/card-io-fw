#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Method {
    Delete,
    Get,
    Head,
    Post,
    Put,
    Connect,
    Options,
    Trace,
    Copy,
    Lock,
    MkCol,
    Move,
    Propfind,
    Proppatch,
    Search,
    Unlock,
    Bind,
    Rebind,
    Unbind,
    Acl,
    Report,
    MkActivity,
    Checkout,
    Merge,
    MSearch,
    Notify,
    Subscribe,
    Unsubscribe,
    Patch,
    Purge,
    MkCalendar,
    Link,
    Unlink,
}

impl Method {
    pub fn new(method: &str) -> Option<Self> {
        let methods = [
            ("Delete", Self::Delete),
            ("Get", Self::Get),
            ("Head", Self::Head),
            ("Post", Self::Post),
            ("Put", Self::Put),
            ("Connect", Self::Connect),
            ("Options", Self::Options),
            ("Trace", Self::Trace),
            ("Copy", Self::Copy),
            ("Lock", Self::Lock),
            ("MkCol", Self::MkCol),
            ("Move", Self::Move),
            ("Propfind", Self::Propfind),
            ("Proppatch", Self::Proppatch),
            ("Search", Self::Search),
            ("Unlock", Self::Unlock),
            ("Bind", Self::Bind),
            ("Rebind", Self::Rebind),
            ("Unbind", Self::Unbind),
            ("Acl", Self::Acl),
            ("Report", Self::Report),
            ("MkActivity", Self::MkActivity),
            ("Checkout", Self::Checkout),
            ("Merge", Self::Merge),
            ("MSearch", Self::MSearch),
            ("Notify", Self::Notify),
            ("Subscribe", Self::Subscribe),
            ("Unsubscribe", Self::Unsubscribe),
            ("Patch", Self::Patch),
            ("Purge", Self::Purge),
            ("MkCalendar", Self::MkCalendar),
            ("Link", Self::Link),
            ("Unlink", Self::Unlink),
        ];

        methods
            .iter()
            .find(|(m, _)| m.eq_ignore_ascii_case(method))
            .map(|(_, m)| *m)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Delete => "DELETE",
            Self::Get => "GET",
            Self::Head => "HEAD",
            Self::Post => "POST",
            Self::Put => "PUT",
            Self::Connect => "CONNECT",
            Self::Options => "OPTIONS",
            Self::Trace => "TRACE",
            Self::Copy => "COPY",
            Self::Lock => "LOCK",
            Self::MkCol => "MKCOL",
            Self::Move => "MOVE",
            Self::Propfind => "PROPFIND",
            Self::Proppatch => "PROPPATCH",
            Self::Search => "SEARCH",
            Self::Unlock => "UNLOCK",
            Self::Bind => "BIND",
            Self::Rebind => "REBIND",
            Self::Unbind => "UNBIND",
            Self::Acl => "ACL",
            Self::Report => "REPORT",
            Self::MkActivity => "MKACTIVITY",
            Self::Checkout => "CHECKOUT",
            Self::Merge => "MERGE",
            Self::MSearch => "MSEARCH",
            Self::Notify => "NOTIFY",
            Self::Subscribe => "SUBSCRIBE",
            Self::Unsubscribe => "UNSUBSCRIBE",
            Self::Patch => "PATCH",
            Self::Purge => "PURGE",
            Self::MkCalendar => "MKCALENDAR",
            Self::Link => "LINK",
            Self::Unlink => "UNLINK",
        }
    }
}
