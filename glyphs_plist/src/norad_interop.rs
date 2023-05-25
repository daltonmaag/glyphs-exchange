use crate::{Anchor, Component, Node, NodeType, Path};

impl From<&norad::Contour> for Path {
    fn from(contour: &norad::Contour) -> Self {
        let mut nodes: Vec<Node> = contour
            .points
            .iter()
            .map(|contour| contour.into())
            .collect();
        if contour.is_closed() {
            // In Glyphs.app, the starting node of a closed contour is
            // always stored at the end of the nodes list.
            nodes.rotate_left(1);
        }
        Self {
            closed: contour.is_closed(),
            nodes,
        }
    }
}

impl From<&Path> for norad::Contour {
    fn from(path: &Path) -> Self {
        let mut points: Vec<norad::ContourPoint> =
            path.nodes.iter().map(|node| node.into()).collect();
        if !path.closed {
            assert!(points[0].typ == norad::PointType::Line && !points[0].smooth);
            points[0].typ = norad::PointType::Move;
        } else {
            // In Glyphs.app, the starting node of a closed contour is
            // always stored at the end of the nodes list.
            points.rotate_right(1);
        }
        Self::new(points, None, None)
    }
}

impl From<&norad::ContourPoint> for Node {
    fn from(point: &norad::ContourPoint) -> Self {
        Self {
            pt: kurbo::Point::new(point.x, point.y),
            node_type: match (&point.typ, point.smooth) {
                (norad::PointType::Move, _) => NodeType::Line,
                (norad::PointType::Line, true) => NodeType::LineSmooth,
                (norad::PointType::Line, false) => NodeType::Line,
                (norad::PointType::OffCurve, _) => NodeType::OffCurve,
                (norad::PointType::Curve, true) => NodeType::CurveSmooth,
                (norad::PointType::Curve, false) => NodeType::Curve,
                (norad::PointType::QCurve, true) => NodeType::QCurveSmooth,
                (norad::PointType::QCurve, false) => NodeType::QCurve,
            },
        }
    }
}

impl From<&Node> for norad::ContourPoint {
    fn from(node: &Node) -> Self {
        let (typ, smooth) = match &node.node_type {
            NodeType::Curve => (norad::PointType::Curve, false),
            NodeType::CurveSmooth => (norad::PointType::Curve, true),
            NodeType::Line => (norad::PointType::Line, false),
            NodeType::LineSmooth => (norad::PointType::Line, true),
            NodeType::OffCurve => (norad::PointType::OffCurve, false),
            NodeType::QCurve => (norad::PointType::QCurve, false),
            NodeType::QCurveSmooth => (norad::PointType::QCurve, true),
        };
        Self::new(node.pt.x, node.pt.y, typ, smooth, None, None, None)
    }
}

impl From<&norad::Component> for Component {
    fn from(component: &norad::Component) -> Self {
        Self {
            name: component.base.to_string(),
            transform: if component.transform == Default::default() {
                None
            } else {
                Some(component.transform.into())
            },
            other_stuff: Default::default(),
        }
    }
}

impl TryFrom<&Component> for norad::Component {
    type Error = norad::error::NamingError;

    fn try_from(component: &Component) -> Result<Self, Self::Error> {
        let name = norad::Name::new(&component.name)?;
        Ok(Self::new(
            name,
            component.transform.unwrap_or_default().into(),
            None,
            None,
        ))
    }
}

impl From<&norad::Anchor> for Anchor {
    fn from(anchor: &norad::Anchor) -> Self {
        Self {
            name: anchor.name.as_ref().unwrap().as_str().to_string(),
            position: kurbo::Point::new(anchor.x, anchor.y),
        }
    }
}

impl TryFrom<&Anchor> for norad::Anchor {
    type Error = norad::error::NamingError;

    fn try_from(anchor: &Anchor) -> Result<Self, Self::Error> {
        let name = norad::Name::new(&anchor.name)?;
        Ok(Self::new(
            anchor.position.x,
            anchor.position.y,
            Some(name),
            None,
            None,
            None,
        ))
    }
}
