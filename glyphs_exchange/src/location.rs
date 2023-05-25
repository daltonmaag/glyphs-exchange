use norad::designspace;

pub struct Location(Vec<f64>);

type LocationTuple = (
    f64,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
);

impl Location {
    // TODO: Fix reliance on the order of dimensions in the location.
    pub fn from_dimension(dimension: &[designspace::Dimension]) -> Self {
        let locations: Vec<_> = dimension
            .iter()
            .map(|dim| dim.xvalue.unwrap_or(0.0) as f64)
            .collect();
        assert!(!locations.is_empty() && locations.len() <= 6);
        Self(locations)
    }

    pub fn as_tuple(&self) -> LocationTuple {
        (
            *self.0.first().unwrap(),
            self.0.get(1).cloned(),
            self.0.get(2).cloned(),
            self.0.get(3).cloned(),
            self.0.get(4).cloned(),
            self.0.get(5).cloned(),
        )
    }
}

/// Render location as a string like Glyphs.app would for brace layers, i.e.
/// "{123, 456}" for a two-axis location.
impl std::fmt::Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{")?;
        for (i, value) in self.0.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{}", value)?;
        }
        write!(f, "}}")
    }
}

// TODO: Add test that any input DS location can roundtrip to Location and back.
