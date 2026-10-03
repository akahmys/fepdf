//! Writing a document out: the copy a save takes, its metadata, encryption, linearisation
//! and signing.

use super::{PdfDocument, SaveOptions, SignOptions, pdf_now, written_version};
use fepdf_model::interpretation::Decision;
use fepdf_model::metadata::MetadataInfo;
use fepdf_model::{Document, PdfError, PdfResult};
use std::path::Path;

impl PdfDocument {
    /// Writes the document, returning what the write cost that the caller must know.
    ///
    /// The `Vec<Decision>` is not decoration. Decryption drops `/Encrypt`, so a
    /// document whose `/P` forbade modification produces output declaring nothing —
    /// and that used to happen in silence. Returning the decisions makes the compiler
    /// ask every caller what it intends to do with them, which is the mechanism
    /// ADR-0005 prefers over remembering: `let _ =` in a test is a caller saying it
    /// does not assert on this, which is honest; a frontend ignoring it is visible in
    /// review because it had to write the discard.
    pub fn save_as_version(&self, output_path: &Path, version: &str) -> PdfResult<Vec<Decision>> {
        self.save_with_options(output_path, version, &SaveOptions::default())
    }

    /// The copy a save writes, as a document of its own, carrying where it came from.
    ///
    /// **The copy, so that saving changes nothing it was not asked to.** The metadata a
    /// save settles — the producer, a title or an author the options give, and the
    /// stripping `strip` asks for — was written into the open document and the copy taken
    /// after, so a stripped export left the document still open with no title or author,
    /// and the next ordinary save wrote none. Nothing recorded it either, so undoing
    /// anything, which replays the history onto the file, brought them back.
    pub(super) fn output_document(&self) -> PdfResult<Document> {
        let mut output = fepdf_doc::assembly::copied(&self.inner)?;
        output.provenance = self.inner.provenance.clone();
        Ok(output)
    }

    /// Saves the document with custom options.
    pub fn save_with_options(
        &self,
        output_path: &Path,
        version: &str,
        options: &SaveOptions,
    ) -> PdfResult<Vec<Decision>> {
        self.write_out(output_path, version, options, None, false)
    }

    /// Applies the metadata the options ask for to `output`, the copy being written,
    /// returning what stripping cost.
    pub(super) fn settle_metadata(
        output: &Document,
        options: &SaveOptions,
    ) -> PdfResult<fepdf_model::interpretation::DecisionLog> {
        let mut metadata = output.metadata();

        if let Some(v) = &options.title {
            metadata.title = Some(v.clone());
        }
        if let Some(v) = &options.author {
            metadata.author = Some(v.clone());
        }
        if let Some(v) = &options.lang {
            metadata.language = Some(v.clone());
        }
        if let Some(v) = &options.copyright {
            metadata.rights = Some(v.clone());
        }
        // **A creation date the caller states is the document's.** It was read by nothing
        // (ROADMAP Y-F2). One that is no date is refused rather than written as one.
        if let Some(v) = &options.creation_date {
            if fepdf_model::refine::metadata::parse_date_string(v).is_none() {
                return Err(PdfError::refused(
                    "save",
                    format!("the creation date {v:?} is no date in either 7.9.4's or XMP's form"),
                ));
            }
            metadata.creation_date = Some(v.clone());
        }
        // **Saving produces a new document** (ADR-0012), so its modification is this save:
        // the moment it is stamped with, in UTC so that one stamp writes one date on any
        // machine. It was the source's, copied (ROADMAP Y-F6).
        metadata.mod_date = Some(utc_date(options.stamp()));

        // Automatic Producer stamping
        metadata.producer = Some("fepdf (https://github.com/akahmys/fepdf)".to_string());

        if options.strip {
            // Strip metadata: we'll clear the fields in the struct
            metadata = MetadataInfo::default();
            metadata.producer = Some("fepdf (optimized)".to_string());
        }

        fepdf_model::metadata::update_document_metadata(output, &metadata, options.stamp())?;

        let mut stripped = fepdf_model::interpretation::DecisionLog::default();
        if options.strip {
            // Every metadata stream, not only the catalogue's. Runs after the write
            // above, which would otherwise put a fresh packet back.
            fepdf_model::metadata::strip_metadata_streams(output, &mut stripped);
        }
        Ok(stripped)
    }

    /// Describes the signature field for this save, from what the caller asked for.
    pub(super) fn signature_field(
        identity: &fepdf_model::cms::SigningIdentity,
        options: &SignOptions,
    ) -> fepdf_model::interactive::SignatureField {
        fepdf_model::interactive::SignatureField {
            page_index: options.page_index,
            field_name: "Signature1".to_string(),
            signed_at: pdf_now(),
            // Table 255 wants /Name only when the signature cannot supply it. The
            // certificate usually can, so this is normally absent.
            signer: identity.common_name().map_or_else(|| options.name.clone(), |_| None),
            reason: options.reason.clone(),
            location: options.location.clone(),
            contact: options.contact_info.clone(),
        }
    }

    /// The one write path, signed or not.
    ///
    /// Signing is not a different way of saving — it is the same file with two fields
    /// filled in afterwards — so the two share this rather than running in parallel. A
    /// signed document produced by a second code path would drift from the unsigned one
    /// silently, and the difference would be invisible until a signature failed to
    /// verify against a file nobody could reproduce.
    pub(super) fn write_out(
        &self,
        output_path: &Path,
        version: &str,
        options: &SaveOptions,
        signing: Option<(&fepdf_model::cms::SigningIdentity, &SignOptions)>,
        linearize: bool,
    ) -> PdfResult<Vec<Decision>> {
        written_version(version)?;
        if options.dry_run {
            // Nothing was written, so nothing was lost.
            return Ok(Vec::new());
        }

        let output = self.output_document()?;
        let stripped = Self::settle_metadata(&output, options)?;
        let mut claims = fepdf_model::interpretation::DecisionLog::default();
        fepdf_model::ingest::conform::drop_subset_claims(output.arena(), &mut claims);
        let (final_arena, root, info) =
            (output.arena(), *output.root_handle(), output.info_handle());

        let file = std::fs::File::create(output_path).map_err(PdfError::Io)?;
        let mut writer = crate::writer::PdfWriter::new(file, final_arena);
        writer.set_string_encoding(options.string_encoding);
        if options.compress {
            writer.set_compression(options.compression_level);
        }
        writer.set_pack_objects(options.obj_stm);
        writer.set_linearize(linearize);
        let mut encryption = Vec::new();
        Self::apply_encryption(&mut writer, options, &mut encryption)?;
        if let Some((identity, sign_options)) = signing {
            // The field goes into the arena that is about to be written, not into the
            // document: what is signed is this output, and nothing about the document
            // this engine holds changes because a copy of it was signed.
            let field = Self::signature_field(identity, sign_options);
            let signature =
                fepdf_model::interactive::add_signature_field(final_arena, root, &field)?;
            writer.sign_with(signature, identity)?;
        }
        writer.write_header(version)?;
        writer.finish(root, info)?;
        let mut decisions = self.write_decisions();
        decisions.extend(stripped.into_entries());
        decisions.extend(claims.into_entries());
        decisions.extend(encryption);
        Ok(decisions)
    }

    /// Encrypts the output, by password or to certificates, or leaves it plain.
    ///
    /// The two are exclusive because a document has one `/Encrypt` and 7.6.4 and 7.6.5
    /// are different handlers. Refusing beats picking one: a caller that asked for both
    /// has a bug, and silently honouring whichever was checked first hides it.
    pub(super) fn apply_encryption<W: std::io::Write>(
        writer: &mut crate::writer::PdfWriter<'_, W>,
        options: &SaveOptions,
        decisions: &mut Vec<Decision>,
    ) -> PdfResult<()> {
        if options.password.is_some() && !options.recipients.is_empty() {
            return Err(PdfError::Crypto(
                "a document is encrypted with a password or to certificates, not both".into(),
            ));
        }
        if let Some(password) = &options.password {
            let (handler, artifacts) = Self::encryption_for(password, options, decisions)?;
            writer.encrypt_with(handler, artifacts);
        } else if !options.recipients.is_empty() {
            let permissions = match &options.permissions {
                Some(list) => fepdf_model::encryption::permissions_from_keywords(list)?,
                // Every bit granted, as for the standard handler: encrypting a document
                // is not on its own a statement about what may be done with it.
                None => -1,
            };
            let (handler, entries) =
                fepdf_model::security::SecurityHandler::encrypt_to_certificates(
                    &options.recipients,
                    permissions,
                    true,
                )?;
            writer.encrypt_to(handler, entries);
        }
        Ok(())
    }

    /// Builds the handler for an encrypted save, recording what the options left open.
    pub(super) fn encryption_for(
        password: &str,
        options: &SaveOptions,
        decisions: &mut Vec<Decision>,
    ) -> PdfResult<(
        fepdf_model::security::SecurityHandler,
        fepdf_model::security::EncryptionArtifacts,
    )> {
        let permissions = match &options.permissions {
            Some(list) => fepdf_model::encryption::permissions_from_keywords(list)?,
            // Every bit granted. `/P` is a declaration this engine reports and does not
            // enforce, and encrypting a document is not on its own a statement about
            // what may be done with it once open.
            None => -1,
        };

        let owner = options.owner_password.as_deref().unwrap_or(password);
        if options.owner_password.is_none() {
            // Worth saying out loud rather than defaulting in silence. The owner
            // password lifts `/P` (7.6.4.1), so making it the open password means the
            // permissions restrict nobody who can open the file — which is exactly the
            // party they were meant to restrict.
            decisions.push(Decision::ambiguity(
                "7.6.4.1",
                "no owner password was given",
                "the open password carries owner rights, so /P restricts nobody who can open it",
            ));
        }

        Ok(fepdf_model::security::SecurityHandler::encrypt_new(password, owner, permissions, true)?)
    }

    /// Saves a linearized (Fast Web View) version of the document with custom options.
    pub fn save_linearized(
        &self,
        output_path: &Path,
        version: &str,
        options: &SaveOptions,
    ) -> PdfResult<Vec<Decision>> {
        // **One way to save** (ROADMAP Y-F1). This kept its own copy of the metadata
        // handling and read `title` and `author` alone, so `strip`, `lang`, `copyright`,
        // `creation_date` and the 2.0 translation of what a save writes did nothing here,
        // and `password` wrote a file unencrypted without a word. It is the save with
        // linearising on: every option means what it means there, and one the
        // linearised layout cannot carry — encryption, a signature — is refused.
        self.write_out(output_path, version, options, None, true)
    }

    /// Signs the document and saves it.
    ///
    /// What is signed is *this output* — the file this call writes, byte for byte — and
    /// not the document that was read. [ADR-0014] is why: normalising at load means the
    /// engine no longer has the source bytes, so signing a document it did not produce
    /// would sign something the user never saw. Signing its own output is exact.
    ///
    /// This wrote `/SubFilter /adbe.pkcs7.detached` with 8,192 zero bytes for
    /// `/Contents` and a `/ByteRange` of four constants, and refused rather than keep
    /// doing so. The refusal stood until the file could carry a signature that covers
    /// it.
    ///
    /// # Errors
    /// If no certificate and key were given, if they cannot be read, or if the write
    /// fails. Both must be DER: a PEM file converts with `openssl x509 -outform der`
    /// and `openssl pkcs8 -topk8 -nocrypt -outform der`.
    ///
    /// [ADR-0014]: ../../../../docs/adr/0014-the-faithful-copy-path-is-not-built.md
    pub fn save_signed(
        &self,
        output_path: &Path,
        version: &str,
        options: &SaveOptions,
        sign_options: &SignOptions,
    ) -> PdfResult<Vec<Decision>> {
        let certificate = sign_options
            .certificate
            .as_deref()
            .ok_or(PdfError::Crypto("signing needs a certificate".into()))?;
        let key = sign_options
            .private_key
            .as_deref()
            .ok_or(PdfError::Crypto("signing needs a private key".into()))?;
        let identity = fepdf_model::cms::SigningIdentity::from_der(certificate, key)?;
        self.write_out(output_path, version, options, Some((&identity, sign_options)), false)
    }
}

/// `seconds` since the Unix epoch as XMP writes a date, in UTC.
fn utc_date(seconds: u64) -> String {
    let at = i64::try_from(seconds).ok().and_then(|s| chrono::DateTime::from_timestamp(s, 0));
    at.unwrap_or_default().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}
