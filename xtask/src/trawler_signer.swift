// Secure-Enclave SSH signer — the bridge stock macOS lacks (and the Secretive
// app exists to provide): mints a Touch-ID-gated P-256 key in the Enclave via
// CryptoKit and emits OpenSSH SSHSIG signatures, so `git commit -S` and
// `ssh-keygen -Y sign`-style callers can use it through `gpg.ssh.program`.
// Anything that is not a sign request for an Enclave key blob is passed
// through to /usr/bin/ssh-keygen, making this a drop-in superset.
//
// Key blob file: line 1 the header below, line 2 the base64 CryptoKit
// dataRepresentation — usable only by this machine's Secure Enclave, and only
// with a fresh user-presence check (Touch ID) per signature.
import CryptoKit
import Foundation
import LocalAuthentication
import Security

let keyBlobHeader = "trawler-se-v1"
let keyType = "ecdsa-sha2-nistp256"

func die(_ msg: String) -> Never {
    FileHandle.standardError.write(Data(("trawler-signer: " + msg + "\n").utf8))
    exit(1)
}

func passthrough(_ args: [String]) -> Never {
    let proc = Process()
    proc.executableURL = URL(fileURLWithPath: "/usr/bin/ssh-keygen")
    proc.arguments = args
    do { try proc.run() } catch { die("cannot run ssh-keygen: \(error)") }
    proc.waitUntilExit()
    exit(proc.terminationStatus)
}

// SSH wire encoding (RFC 4251) and the SSHSIG container (openssh PROTOCOL.sshsig).
func u32(_ n: UInt32) -> Data { withUnsafeBytes(of: n.bigEndian) { Data($0) } }
func sshString(_ d: Data) -> Data { u32(UInt32(d.count)) + d }
func sshString(_ s: String) -> Data { sshString(Data(s.utf8)) }

func mpint(_ raw: Data) -> Data {
    var bytes = [UInt8](raw)
    while bytes.first == 0 { bytes.removeFirst() }
    if let first = bytes.first, first & 0x80 != 0 { bytes.insert(0, at: 0) }
    return sshString(Data(bytes))
}

func pubkeyWire(_ key: SecureEnclave.P256.Signing.PrivateKey) -> Data {
    sshString(keyType) + sshString("nistp256") + sshString(key.publicKey.x963Representation)
}

func keygen(stem: String) {
    var accessError: Unmanaged<CFError>?
    guard
        let access = SecAccessControlCreateWithFlags(
            kCFAllocatorDefault,
            kSecAttrAccessibleWhenUnlockedThisDeviceOnly,
            [.privateKeyUsage, .userPresence],
            &accessError
        )
    else { die("access control: \(accessError!.takeRetainedValue())") }
    let key: SecureEnclave.P256.Signing.PrivateKey
    do {
        key = try SecureEnclave.P256.Signing.PrivateKey(accessControl: access)
    } catch { die("Secure Enclave key generation failed: \(error)") }
    do {
        let blob = keyBlobHeader + "\n" + key.dataRepresentation.base64EncodedString() + "\n"
        try blob.write(toFile: stem, atomically: true, encoding: .utf8)
        try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: stem)
        let pub = "\(keyType) \(pubkeyWire(key).base64EncodedString()) trawler-se\n"
        try pub.write(toFile: stem + ".pub", atomically: true, encoding: .utf8)
    } catch { die("cannot write key files: \(error)") }
    print(stem + ".pub")
}

func loadKey(_ blobFile: String, _ content: String) -> SecureEnclave.P256.Signing.PrivateKey {
    let lines = content.split(separator: "\n", omittingEmptySubsequences: true)
    guard
        lines.count >= 2,
        let blob = Data(base64Encoded: String(lines[1]))
    else { die("malformed key blob at \(blobFile)") }
    let ctx = LAContext()
    ctx.localizedReason =
        ProcessInfo.processInfo.environment["TRAWLER_SIGN_REASON"]
        ?? "sign for Trawler manual review"
    do {
        return try SecureEnclave.P256.Signing.PrivateKey(
            dataRepresentation: blob, authenticationContext: ctx)
    } catch { die("cannot load Secure Enclave key (created on another machine?): \(error)") }
}

func sign(namespace: String, key: SecureEnclave.P256.Signing.PrivateKey, message: Data) -> String {
    var payload = Data("SSHSIG".utf8)
    payload += sshString(namespace) + sshString("") + sshString("sha512")
    payload += sshString(Data(SHA512.hash(data: message)))
    let sig: P256.Signing.ECDSASignature
    do {
        sig = try key.signature(for: payload)
    } catch { die("Secure Enclave signing failed (Touch ID declined or timed out?): \(error)") }
    let raw = sig.rawRepresentation
    let sigWire = sshString(keyType) + sshString(mpint(raw.prefix(32)) + mpint(raw.suffix(32)))
    var blob = Data("SSHSIG".utf8) + u32(1)
    blob += sshString(pubkeyWire(key)) + sshString(namespace) + sshString("")
    blob += sshString("sha512") + sshString(sigWire)
    let b64 = blob.base64EncodedString()
    var lines: [String] = []
    var idx = b64.startIndex
    while idx < b64.endIndex {
        let end = b64.index(idx, offsetBy: 70, limitedBy: b64.endIndex) ?? b64.endIndex
        lines.append(String(b64[idx..<end]))
        idx = end
    }
    return "-----BEGIN SSH SIGNATURE-----\n" + lines.joined(separator: "\n")
        + "\n-----END SSH SIGNATURE-----\n"
}

let args = Array(CommandLine.arguments.dropFirst())
if args.first == "keygen" {
    guard args.count == 2 else { die("usage: trawler-signer keygen <stem>") }
    keygen(stem: args[1])
    exit(0)
}
if args.first == "check" {
    let enclave = SecureEnclave.isAvailable
    let biometry = LAContext().canEvaluatePolicy(
        .deviceOwnerAuthenticationWithBiometrics, error: nil)
    print("enclave=\(enclave ? "yes" : "no") biometry=\(biometry ? "yes" : "no")")
    exit(0)
}

func flagValue(_ args: [String], _ i: inout Int) -> String {
    i += 1
    guard i < args.count else { die("missing value for \(args[i - 1])") }
    return args[i]
}

var mode: String?
var namespace = "file"
var keyPath: String?
var files: [String] = []
var i = 0
while i < args.count {
    switch args[i] {
    case "-Y": mode = flagValue(args, &i)
    case "-n": namespace = flagValue(args, &i)
    case "-f": keyPath = flagValue(args, &i)
    case "-O": _ = flagValue(args, &i)
    case "-U", "-q": break
    default: files.append(args[i])
    }
    i += 1
}

guard mode == "sign", var blobFile = keyPath else { passthrough(args) }
if blobFile.hasSuffix(".pub") {
    let stem = String(blobFile.dropLast(4))
    if FileManager.default.fileExists(atPath: stem) { blobFile = stem }
}
guard
    let content = try? String(contentsOfFile: blobFile, encoding: .utf8),
    content.hasPrefix(keyBlobHeader + "\n")
else { passthrough(args) }

let key = loadKey(blobFile, content)
if files.isEmpty || files == ["-"] {
    let message = FileHandle.standardInput.readDataToEndOfFile()
    print(sign(namespace: namespace, key: key, message: message), terminator: "")
} else {
    for file in files {
        guard let message = FileManager.default.contents(atPath: file) else {
            die("cannot read \(file)")
        }
        let armored = sign(namespace: namespace, key: key, message: message)
        do { try armored.write(toFile: file + ".sig", atomically: true, encoding: .utf8) } catch {
            die("cannot write \(file).sig: \(error)")
        }
    }
}
