//! A conexão criptografada (TLS) do Postgres com as opções da conexão: conferir ou não o
//! certificado do servidor, a autoridade (CA) da empresa e o certificado do cliente.
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};

use crate::Conexao;

fn ler(caminho: &str, o_que: &str) -> Result<Vec<u8>, String> {
    std::fs::read(caminho.trim()).map_err(|e| format!("{o_que} ({caminho}): {e}"))
}

/// Os certificados de um arquivo PEM (ou um DER sozinho).
pub(crate) fn certificados(caminho: &str, o_que: &str) -> Result<Vec<CertificateDer<'static>>, String> {
    let bytes = ler(caminho, o_que)?;
    if !bytes.starts_with(b"-----") {
        return Ok(vec![CertificateDer::from(bytes)]);
    }
    let v: Vec<_> = CertificateDer::pem_slice_iter(&bytes).collect::<Result<_, _>>().map_err(|e| format!("{o_que} ({caminho}): {e:?}"))?;
    if v.is_empty() {
        return Err(format!("{o_que} ({caminho}): nenhum certificado no arquivo"));
    }
    Ok(v)
}

/// Aceita o certificado que o servidor mandar (modo "só criptografar", o `sslmode=require` do Postgres).
/// A assinatura do aperto de mão continua conferida; só a identidade do servidor não.
#[derive(Debug)]
struct SemConferir(Arc<CryptoProvider>);

impl ServerCertVerifier for SemConferir {
    fn verify_server_cert(&self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn verify_tls13_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

pub(crate) fn config(c: &Conexao) -> Result<rustls::ClientConfig, String> {
    let provedor = Arc::new(rustls::crypto::ring::default_provider());
    let base = rustls::ClientConfig::builder_with_provider(provedor.clone()).with_safe_default_protocol_versions().map_err(|e| e.to_string())?;
    let base = if c.tls_conferir {
        // As autoridades públicas, mais a da empresa quando a pessoa escolheu o arquivo.
        let mut raizes = rustls::RootCertStore { roots: webpki_roots::TLS_SERVER_ROOTS.to_vec() };
        if !c.tls_ca.trim().is_empty() {
            for cert in certificados(&c.tls_ca, "Certificado da autoridade (CA)")? {
                raizes.add(cert).map_err(|e| format!("Certificado da autoridade (CA): {e}"))?;
            }
        }
        base.with_root_certificates(raizes)
    } else {
        base.dangerous().with_custom_certificate_verifier(Arc::new(SemConferir(provedor)))
    };
    if c.tls_cert.trim().is_empty() {
        return Ok(base.with_no_client_auth());
    }
    let cadeia = certificados(&c.tls_cert, "Certificado do cliente")?;
    let chave = PrivateKeyDer::from_pem_slice(&ler(&c.tls_chave, "Chave do cliente")?).map_err(|e| format!("Chave do cliente ({}): {e:?}", c.tls_chave))?;
    base.with_client_auth_cert(cadeia, chave).map_err(|e| format!("Certificado do cliente: {e}"))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn arquivo_que_falta_diz_qual() {
        let c = Conexao { tls: true, tls_ca: "/nao/existe/ca.pem".into(), ..Conexao::nova(crate::Tipo::Postgres) };
        assert!(config(&c).unwrap_err().starts_with("Certificado da autoridade (CA) (/nao/existe/ca.pem)"));
        let c = Conexao { tls: true, tls_conferir: false, ..Conexao::nova(crate::Tipo::Postgres) };
        assert!(config(&c).is_ok());
    }
}
