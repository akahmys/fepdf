//! Integration tests for PDF Security Handlers & Encryption

use ferruginous_core::security::SecurityHandler;

#[test]
fn test_security_handler_v5_aes256() {
    let file_id = b"0123456789abcdef";
    let handler = SecurityHandler::new_v5("secret_password", "owner_secret", file_id);
    assert!(handler.is_ok());
    let h = handler.unwrap();
    assert!(h.should_decrypt_metadata());
}

#[test]
fn test_security_handler_v4_aes128() {
    let file_id = b"0123456789abcdef";
    let dummy_o = [0u8; 32];
    let dummy_u = [0u8; 32];
    let handler = SecurityHandler::new_v4("userpass", &dummy_o, &dummy_u, 128, file_id, true);
    assert!(handler.is_ok());
}
