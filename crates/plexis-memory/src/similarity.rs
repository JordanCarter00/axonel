use crate::error::MemoryError;

/// Computes the dot product of two vectors of equal length.
pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Computes the Euclidean (L2) norm of a vector.
pub fn l2_norm(v: &[f32]) -> f32 {
    v.iter().map(|x| x * x).sum::<f32>().sqrt()
}

/// Normalizes a vector in-place so its L2 norm is 1.0.
/// If the vector is a zero vector, it remains zero.
pub fn l2_normalize(v: &mut [f32]) {
    let norm = l2_norm(v);
    if norm > 1e-12 {
        let inv_norm = 1.0 / norm;
        for x in v.iter_mut() {
            *x *= inv_norm;
        }
    }
}

/// Computes the cosine similarity between two vectors.
/// Returns a value between -1.0 and 1.0 (or 0.0 to 1.0 for normalized non-negative vectors).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> Result<f32, MemoryError> {
    if a.len() != b.len() {
        return Err(MemoryError::DimensionMismatch {
            expected: a.len(),
            actual: b.len(),
        });
    }

    let norm_a = l2_norm(a);
    let norm_b = l2_norm(b);

    if norm_a <= 1e-12 || norm_b <= 1e-12 {
        return Ok(0.0);
    }

    let dot = dot_product(a, b);
    let sim = dot / (norm_a * norm_b);
    // Clamp to [-1.0, 1.0] to guard against floating-point inaccuracies
    Ok(sim.clamp(-1.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity() {
        let v1 = vec![1.0, 0.0, 0.0];
        let v2 = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&v1, &v2).unwrap() - 1.0).abs() < 1e-6);

        let v3 = vec![0.0, 1.0, 0.0];
        assert!((cosine_similarity(&v1, &v3).unwrap() - 0.0).abs() < 1e-6);

        let v4 = vec![-1.0, 0.0, 0.0];
        assert!((cosine_similarity(&v1, &v4).unwrap() - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn test_l2_normalize() {
        let mut v = vec![3.0, 4.0];
        l2_normalize(&mut v);
        assert!((l2_norm(&v) - 1.0).abs() < 1e-6);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((v[1] - 0.8).abs() < 1e-6);
    }
}
