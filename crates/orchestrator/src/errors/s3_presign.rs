#[derive(Debug, thiserror::Error)]
pub enum RsS3PresignError {
    #[error("invalid presigning config: {0}")]
    Config(#[from] aws_sdk_s3::presigning::PresigningConfigError),
    #[error("failed to presign request: {0}")]
    Presign(Box<aws_sdk_s3::error::SdkError<aws_sdk_s3::operation::put_object::PutObjectError>>),
}
