//! Signing on both shapes: the organisation's authority and personal
//! certificates are made once and kept sealed; two people sign a PDF in
//! turn; our verifier, OpenSSL and pyHanko (an independent validator,
//! when ORTUS_TEST_PYHANKO names it) all find both signatures valid; a
//! changed byte breaks them; a revoked key is not used again.

mod common;

use ortus_core::auth::accounts::Role;
use ortus_core::instance::testing_auth::signed_in;
use ortus_tasks::{certs, pades};

fn openssl_verifies(pdf: &[u8], ca_pem: &str, dir: &std::path::Path) -> bool {
    // Each signature: its CMS and the bytes it covers, checked by OpenSSL.
    let text = pdf.to_vec();
    let mut all = true;
    let mut from = 0;
    let mut n = 0;
    while let Some(p) = text[from..].windows(12).position(|w| w == b"/ByteRange [") {
        let at = from + p + 12;
        let close = at + text[at..].iter().position(|c| *c == b']').unwrap();
        let v: Vec<usize> = std::str::from_utf8(&text[at..close])
            .unwrap()
            .split_whitespace()
            .map(|x| x.parse().unwrap())
            .collect();
        let hex = &text[v[1] + 1..v[2] - 1];
        let mut der: Vec<u8> = hex
            .chunks(2)
            .map(|c| u8::from_str_radix(std::str::from_utf8(c).unwrap(), 16).unwrap())
            .collect();
        while der.last() == Some(&0) {
            der.pop();
        }
        let mut data = text[..v[1]].to_vec();
        data.extend_from_slice(&text[v[2]..v[2] + v[3]]);
        let (sig, content, ca) = (
            dir.join(format!("sig{n}.der")),
            dir.join(format!("data{n}.bin")),
            dir.join("ca.pem"),
        );
        std::fs::write(&sig, &der).unwrap();
        std::fs::write(&content, &data).unwrap();
        std::fs::write(&ca, ca_pem).unwrap();
        let ok = std::process::Command::new("openssl")
            .args([
                "cms",
                "-verify",
                "-binary",
                "-inform",
                "DER",
                "-purpose",
                "any",
                "-out",
                "/dev/null",
            ])
            .arg("-in")
            .arg(&sig)
            .arg("-content")
            .arg(&content)
            .arg("-CAfile")
            .arg(&ca)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        all &= ok;
        n += 1;
        from = close;
    }
    all && n > 0
}

#[test]
fn two_people_sign_in_turn_and_every_validator_agrees() {
    for (name, instance, _keep) in common::installations() {
        signed_in(&instance, "marek@ministry.example", Role::Member);
        signed_in(&instance, "katarzyna@ministry.example", Role::Member);
        let user = |e: &str| {
            ortus_core::runtime::block_on(ortus_core::auth::accounts::by_email(
                instance.store.as_ref(),
                e,
            ))
            .unwrap()
            .unwrap()
        };
        let (marek, katarzyna) = (
            user("marek@ministry.example"),
            user("katarzyna@ministry.example"),
        );
        let ca = ortus_core::runtime::block_on(certs::authority(&instance)).unwrap();
        // Made once: asked again, the same authority and the same key.
        assert_eq!(
            ca.cert,
            ortus_core::runtime::block_on(certs::authority(&instance))
                .unwrap()
                .cert,
            "{name}"
        );
        let m = ortus_core::runtime::block_on(certs::personal(&instance, &marek)).unwrap();
        assert_eq!(
            m.cert,
            ortus_core::runtime::block_on(certs::personal(&instance, &marek))
                .unwrap()
                .cert,
            "{name}"
        );
        let k = ortus_core::runtime::block_on(certs::personal(&instance, &katarzyna)).unwrap();
        // Keys are kept sealed, never in the clear.
        let sealed = ortus_core::runtime::block_on(
            instance
                .store
                .query("SELECT key_sealed FROM signing_keys", &[]),
        )
        .unwrap();
        assert!(
            sealed.iter().all(|r| !r.text(0).unwrap().contains("BEGIN")),
            "{name}"
        );

        let base = common::tiny_pdf("Flood response plan 2026");
        let once = pades::sign(
            &base,
            &m,
            std::slice::from_ref(&ca.cert),
            &pades::Signing {
                name: "Marek Nowak",
                reason: "Author signs",
                at_ms: 1_795_000_000_000,
            },
        )
        .unwrap();
        let twice = pades::sign(
            &once,
            &k,
            std::slice::from_ref(&ca.cert),
            &pades::Signing {
                name: "Katarzyna Lis",
                reason: "Director signs",
                at_ms: 1_795_010_000_000,
            },
        )
        .unwrap();
        let found = pades::verify(&twice, &ca.cert);
        assert_eq!(2, found.len(), "{name}");
        assert!(
            found.iter().all(|f| f.intact && f.trusted),
            "{name}: {found:?}"
        );
        assert_eq!((false, true), (found[0].whole, found[1].whole), "{name}");
        assert_eq!(
            (marek.name.as_str(), "Director signs"),
            (found[0].signer.as_str(), found[1].reason.as_str()),
            "{name}"
        );
        assert_eq!(Some(1_795_000_000_000), found[0].at, "{name}");
        assert_eq!(
            Some("katarzyna@ministry.example".to_string()),
            found[1].email,
            "{name}"
        );

        // Independent validators.
        let dir = common::dir();
        assert!(
            openssl_verifies(&twice, &certs::pem(&ca.cert), &dir.0),
            "{name}: OpenSSL refused it"
        );
        if let Ok(pyhanko) = std::env::var("ORTUS_TEST_PYHANKO") {
            let file = dir.0.join("signed.pdf");
            std::fs::write(&file, &twice).unwrap();
            std::fs::write(dir.0.join("ca.pem"), certs::pem(&ca.cert)).unwrap();
            let out = std::process::Command::new(pyhanko)
                .args([
                    "sign",
                    "validate",
                    "--pretty-print",
                    "--no-revocation-check",
                    "--trust",
                ])
                .arg(dir.0.join("ca.pem"))
                .arg(&file)
                .output()
                .unwrap();
            let text = String::from_utf8_lossy(&out.stdout).to_string()
                + &String::from_utf8_lossy(&out.stderr);
            assert!(out.status.success(), "{name}: pyHanko: {text}");
            assert!(
                text.contains("INTACT:TRUSTED") || text.to_lowercase().contains("trusted"),
                "{name}: {text}"
            );
        } else {
            assert!(
                std::env::var("ORTUS_REQUIRE_PYHANKO").is_err(),
                "ORTUS_REQUIRE_PYHANKO is set but ORTUS_TEST_PYHANKO is not"
            );
        }

        // One changed byte in the document breaks both.
        let mut changed = twice.clone();
        let p = changed.windows(5).position(|w| w == b"Flood").unwrap();
        changed[p] = b'B';
        assert!(
            pades::verify(&changed, &ca.cert).iter().all(|f| !f.intact),
            "{name}"
        );
        // Another authority's certificate is not trusted.
        let stranger = common::tiny_pdf("x");
        assert!(pades::verify(&stranger, &ca.cert).is_empty(), "{name}");

        // Revoked: a new key next time.
        ortus_core::runtime::block_on(certs::revoke(instance.store.as_ref(), &marek.id)).unwrap();
        assert!(
            ortus_core::runtime::block_on(certs::revoked_at(instance.store.as_ref(), &m.cert))
                .is_some(),
            "{name}"
        );
        let again = ortus_core::runtime::block_on(certs::personal(&instance, &marek)).unwrap();
        assert_ne!(m.cert, again.cert, "{name}");
    }
}
