use crate::error::Error;

use super::*;
use blue_hal::{
    hal::flash,
    utilities::{iterator::UntilSequence, memory::Address},
};

pub use ::ecdsa::{elliptic_curve::generic_array::typenum::Unsigned, SignatureSize};
pub use ecdsa::signature::Signature as EcdsaSignature;
use nb::block;
use p256::EncodedPoint;
pub use p256::{
    ecdsa::{signature::DigestVerifier, Signature, VerifyingKey},
    NistP256,
};
pub use sha2::Digest;

fn retrieve_key() -> VerifyingKey {
    #[allow(unused)]
    use core::str::FromStr;

    #[cfg(test)]
    return VerifyingKey::from_str(include_str!("../assets/test_key.pem"))
        .expect("Invalic public key supplied on compilation");

    #[cfg(not(test))]
    return VerifyingKey::from_encoded_point(
        &EncodedPoint::from_bytes(include_bytes!("../assets/key.sec1"))
            .expect("Invalic public key supplied on compilation"),
    )
    .expect("Invalic public key supplied on compilation");
}

pub struct EcdsaImageReader;

impl Reader for EcdsaImageReader {
    fn image_at<A, F>(flash: &mut F, bank: Bank<A>) -> Result<Image<A>, error::Error>
    where
        A: Address,
        F: flash::ReadWrite<Address = A>,
        error::Error: From<F::Error>,
    {
        // Development build shorcut: We're checking that the image does *not* start with 0xFF. This
        // will not be part of the final Loadstone release build, but it helps speed up the
        // verification for invalid images during development.
        if flash.bytes(bank.location).next().ok_or(Error::BankInvalid)? == 0xFF {
            return Err(Error::BankEmpty);
        }
        let key = retrieve_key();

        // Generic buffer to hold temporary slices read from flash memory.
        const BUFFER_SIZE: usize = 256;
        let mut buffer = [0u8; BUFFER_SIZE];

        let (mut digest, mut image_size) = flash
            .bytes(bank.location)
            .take(bank.size)
            .until_sequence(&magic_string_inverted())
            .fold((sha2::Sha256::default(), 0usize), |(mut digest, mut byte_count), byte| {
                digest.update(&[byte]);
                byte_count += 1;
                (digest, byte_count)
            });

        if image_size == bank.size {
            return Err(Error::BankEmpty);
        }

        // Magic string is part of the digest
        digest.update(&magic_string_inverted());

        let signature_position = bank.location + image_size + MAGIC_STRING.len();
        let signature_bytes = &mut buffer[0..SignatureSize::<NistP256>::to_usize()];
        block!(flash.read(signature_position, signature_bytes))?;

        let signature =
            Signature::from_bytes(signature_bytes).map_err(|_| Error::SignatureInvalid)?;
        key.verify_digest(digest, &signature).map_err(|_| Error::SignatureInvalid)?;

        let golden_string_position = bank.location + image_size.saturating_sub(GOLDEN_STRING.len());
        let golden_bytes = &mut buffer[0..GOLDEN_STRING.len()];
        block!(flash.read(golden_string_position, golden_bytes))?;
        let golden = golden_bytes == GOLDEN_STRING.as_bytes();

        if golden {
            image_size = image_size.saturating_sub(GOLDEN_STRING.len());
        }

        Ok(Image {
            size: image_size,
            location: bank.location,
            bootable: bank.bootable,
            golden,
            signature,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use blue_hal::hal::{
        doubles::{
            error::FakeError,
            flash::{Address, FakeFlash},
        },
        flash::ReadWrite,
    };
    use std::convert::TryInto;

    #[rustfmt::skip]
    const TEST_SIGNED_IMAGE: &[u8] = &[
        // Image
        0xaa, 0xbb,
        // Magic string inverted
        0xb7, 0xac, 0x9c, 0xc8, 0x9c, 0xcd, 0x8f, 0x8b,
        0x86, 0x9b, 0xa5, 0xb7, 0xcd, 0xae, 0x94, 0x8e,
        // Signature
        0xa5, 0xa8, 0xaf, 0x9c, 0xb5, 0x98, 0xb8, 0xcc,
        0xb5, 0x8b, 0x91, 0xb5, 0xc9, 0xa9, 0x8a, 0xbe,
        0x49, 0xdb, 0xc3, 0x82, 0x37, 0xff, 0x13, 0x9a,
        0x96, 0xb1, 0xb2, 0x37, 0x4a, 0x41, 0x35, 0x36,
        0xd4, 0xed, 0xc7, 0xdf, 0x00, 0x80, 0x54, 0xde,
        0x95, 0xbe, 0xc5, 0x1b, 0xbb, 0x89, 0xa9, 0x35,
        0x03, 0x62, 0xb0, 0xef, 0x73, 0x1f, 0x32, 0x4a,
        0x5e, 0x93, 0x8c, 0x78, 0x4e, 0xf5, 0x6a, 0x3f,
        0xf5, 0x8f, 0x99, 0xf6, 0x11, 0x67, 0xa6, 0xc2,
        0x12, 0xc7, 0xf5, 0xb3, 0x3b, 0xb0, 0x12, 0x8e,
    ];

    #[rustfmt::skip]
    const TEST_SIGNED_GOLDEN_IMAGE: &[u8] = &[
        // Image
        0xaa, 0xbb,
        // Golden String
        0x58, 0x50, 0x49, 0x63, 0x62, 0x4f, 0x55, 0x72, 0x70, 0x47,
        // Magic String Inverted
        0xb7, 0xac, 0x9c, 0xc8, 0x9c, 0xcd, 0x8f, 0x8b,
        0x86, 0x9b, 0xa5, 0xb7, 0xcd, 0xae, 0x94, 0x8e,
        // Signature
        0xa5, 0xa8, 0xaf, 0x9c, 0xb5, 0x98, 0xb8, 0xcc,
        0xb5, 0x8b, 0x91, 0xb5, 0xc9, 0xa9, 0x8a, 0xbe,
        0x8a, 0xb7, 0xcb, 0x03, 0x03, 0x53, 0xd2, 0xa3,
        0x9d, 0x42, 0x99, 0x3f, 0x94, 0xfc, 0x2d, 0x91,
        0x4b, 0x91, 0x50, 0xfb, 0xdc, 0x28, 0xaa, 0x11,
        0x31, 0xca, 0x4b, 0x4f, 0x74, 0x94, 0xe4, 0xeb,
        0x42, 0x93, 0x24, 0xd1, 0x73, 0x85, 0xcd, 0xd8,
        0x1f, 0x12, 0xbe, 0xcd, 0x4b, 0xdb, 0x9f, 0xcb,
        0x58, 0x0e, 0xef, 0xc6, 0x9e, 0xf2, 0xa3, 0x0e,
        0x7f, 0xa8, 0xbb, 0xf1, 0x26, 0x30, 0xec, 0x5a
    ];

    #[rustfmt::skip]
    const TEST_IMAGE_SIGNED_BY_ANOTHER_KEY: &[u8] = &[
        // Image
        0xaa, 0xbb,

        // Magic string inverted
        0xb7, 0xac, 0x9c, 0xc8, 0x9c, 0xcd, 0x8f, 0x8b,
        0x86, 0x9b, 0xa5, 0xb7, 0xcd, 0xae, 0x94, 0x8e,

        // Signature
        0xa5, 0xa8, 0xaf, 0x9c, 0xb5, 0x98, 0xb8, 0xcc,
        0xb5, 0x8b, 0x91, 0xb5, 0xc9, 0xa9, 0x8a, 0xbe,
        0x12, 0x77, 0x26, 0xc9, 0x13, 0x89, 0x38, 0xca,
        0x23, 0xb9, 0x3d, 0xc9, 0xdc, 0xad, 0xbc, 0x8b,
        0x41, 0x99, 0xe0, 0x89, 0x97, 0xf4, 0x7d, 0x88,
        0xaf, 0xc7, 0x8a, 0x5d, 0xf5, 0xaf, 0x37, 0xdd,
        0x45, 0x0e, 0x38, 0xdc, 0x74, 0x85, 0x72, 0x28,
        0x28, 0x54, 0x15, 0xdd, 0x15, 0x6c, 0x1b, 0x22,
        0xfe, 0x18, 0x40, 0x88, 0xcb, 0x26, 0x4e, 0x22,
        0x3b, 0x0a, 0xbd, 0x09, 0x73, 0x1d, 0x1b, 0x35,
    ];

    #[rustfmt::skip]
    const TEST_GOLDEN_IMAGE_SIGNED_BY_ANOTHER_KEY: &[u8] = &[
        // Image
        0xaa, 0xbb,
        // Golden string
        0x58, 0x50, 0x49, 0x63, 0x62, 0x4f, 0x55, 0x72, 0x70, 0x47,
        // Magic string inverted
        0xb7, 0xac, 0x9c, 0xc8, 0x9c, 0xcd, 0x8f, 0x8b,
        0x86, 0x9b, 0xa5, 0xb7, 0xcd, 0xae, 0x94, 0x8e,
        // Signature
        0xa5, 0xa8, 0xaf, 0x9c, 0xb5, 0x98, 0xb8, 0xcc,
        0xb5, 0x8b, 0x91, 0xb5, 0xc9, 0xa9, 0x8a, 0xbe,
        0xcf, 0x71, 0x77, 0x7f, 0x47, 0x4b, 0x3e, 0xd4,
        0x01, 0xaa, 0x65, 0x22, 0x78, 0x4a, 0x0f, 0x4a,
        0x84, 0x11, 0x65, 0xba, 0x7c, 0x85, 0x00, 0x8b,
        0x9c, 0x87, 0x78, 0xb3, 0x47, 0x36, 0xe8, 0x4d,
        0xb9, 0x24, 0x9f, 0x51, 0x2b, 0x34, 0x2f, 0x70,
        0x75, 0xe7, 0xdf, 0x77, 0x5e, 0x23, 0x8e, 0x92,
        0xf4, 0xe8, 0x3f, 0x79, 0xc2, 0xa3, 0x50, 0x5a,
        0xc7, 0x62, 0x74, 0x6e, 0xd2, 0x0b, 0x96, 0x84
    ];

    #[test]
    fn retrieving_signed_image_succeeds() {
        let mut flash = FakeFlash::new(Address(0));
        let bank =
            Bank { index: 1, size: 512, location: Address(0), bootable: false, is_golden: false };
        flash.write(Address(0), &TEST_SIGNED_IMAGE).unwrap();

        let image = EcdsaImageReader::image_at(&mut flash, bank).unwrap();
        assert_eq!(image.size, 2usize);
        assert_eq!(image.location, bank.location);
        assert_eq!(image.bootable, false);
        assert_eq!(image.is_golden(), false);
    }

    #[test]
    fn retrieving_signed_golden_key_succeeds() {
        let mut flash = FakeFlash::new(Address(0));
        let bank =
            Bank { index: 1, size: 512, location: Address(0), bootable: false, is_golden: false };
        flash.write(Address(0), &TEST_SIGNED_GOLDEN_IMAGE).unwrap();

        let image = EcdsaImageReader::image_at(&mut flash, bank).unwrap();
        assert_eq!(image.size, 2usize);
        assert_eq!(image.location, bank.location);
        assert_eq!(image.bootable, false);
        assert_eq!(image.is_golden(), true);
    }

    #[test]
    fn retrieving_images_signed_by_another_key_fails() {
        let mut flash = FakeFlash::new(Address(0));
        let bank =
            Bank { index: 1, size: 512, location: Address(0), bootable: false, is_golden: false };

        flash.write(Address(0), &TEST_IMAGE_SIGNED_BY_ANOTHER_KEY).unwrap();
        assert_eq!(Err(Error::SignatureInvalid), EcdsaImageReader::image_at(&mut flash, bank));

        flash.write(Address(0), &TEST_GOLDEN_IMAGE_SIGNED_BY_ANOTHER_KEY).unwrap();
        assert_eq!(Err(Error::SignatureInvalid), EcdsaImageReader::image_at(&mut flash, bank));
    }

    #[test]
    fn retrieving_broken_image_fails() {
        let mut flash = FakeFlash::new(Address(0));
        let bank =
            Bank { index: 1, size: 512, location: Address(0), bootable: false, is_golden: false };

        let mut image: [u8; 98] = TEST_SIGNED_IMAGE.try_into().unwrap();
        image[0] = 0xCC; // Corrupted image body;
        flash.write(Address(0), &image).unwrap();
        assert_eq!(Err(Error::SignatureInvalid), EcdsaImageReader::image_at(&mut flash, bank));

        let mut image: [u8; 98] = TEST_SIGNED_IMAGE.try_into().unwrap();
        image[3] = 0xCC; // Corrupted magic string
        flash.write(Address(0), &image).unwrap();
        assert_eq!(Err(Error::BankEmpty), EcdsaImageReader::image_at(&mut flash, bank));

        let mut image: [u8; 98] = TEST_SIGNED_IMAGE.try_into().unwrap();
        image[96] = 0xCC; // Corrupted signature
        flash.write(Address(0), &image).unwrap();
        assert_eq!(Err(Error::SignatureInvalid), EcdsaImageReader::image_at(&mut flash, bank));
    }
}
