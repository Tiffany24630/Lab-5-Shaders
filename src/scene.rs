use crate::math::{Mat4, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyId {
    Solaris,
    Vesper,
    Selene,
    Nymara,
    Cryon,
    Veridis,
    Umbra,
    Solene,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyPose {
    pub id: BodyId,
    pub model_matrix: Mat4,
    pub world_position: Vec3,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BodyPoses {
    poses: [BodyPose; 8],
}

impl BodyPoses {
    pub fn new(poses: [BodyPose; 8]) -> Self {
        Self { poses }
    }

    pub fn get(&self, id: BodyId) -> BodyPose {
        self.poses[id.index()]
    }

    pub fn all(&self) -> &[BodyPose; 8] {
        &self.poses
    }
}

impl BodyId {
    const fn index(self) -> usize {
        match self {
            Self::Solaris => 0,
            Self::Vesper => 1,
            Self::Selene => 2,
            Self::Nymara => 3,
            Self::Cryon => 4,
            Self::Veridis => 5,
            Self::Umbra => 6,
            Self::Solene => 7,
        }
    }
}