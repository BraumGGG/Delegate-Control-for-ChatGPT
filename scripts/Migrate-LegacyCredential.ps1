$ErrorActionPreference = 'Stop'

$legacyPath = 'D:\tunnel-client\install\chatgpt-delegate-control\runtime-api-key.xml'
if (-not (Test-Path -LiteralPath $legacyPath)) {
    throw "找不到旧的加密密钥文件：$legacyPath"
}

Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class DelegateCredentialWriter
{
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    private struct CREDENTIAL
    {
        public UInt32 Flags;
        public UInt32 Type;
        public IntPtr TargetName;
        public IntPtr Comment;
        public System.Runtime.InteropServices.ComTypes.FILETIME LastWritten;
        public UInt32 CredentialBlobSize;
        public IntPtr CredentialBlob;
        public UInt32 Persist;
        public UInt32 AttributeCount;
        public IntPtr Attributes;
        public IntPtr TargetAlias;
        public IntPtr UserName;
    }

    [DllImport("Advapi32.dll", EntryPoint = "CredWriteW", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern bool CredWrite(ref CREDENTIAL credential, UInt32 flags);

    public static void Write(string secret)
    {
        byte[] blob = Encoding.Unicode.GetBytes(secret);
        IntPtr target = Marshal.StringToCoTaskMemUni("RuntimeApiKey.DelegateControl");
        IntPtr username = Marshal.StringToCoTaskMemUni("RuntimeApiKey");
        IntPtr blobPointer = Marshal.AllocCoTaskMem(blob.Length);
        try
        {
            Marshal.Copy(blob, 0, blobPointer, blob.Length);
            CREDENTIAL credential = new CREDENTIAL
            {
                Type = 1,
                TargetName = target,
                CredentialBlobSize = (UInt32)blob.Length,
                CredentialBlob = blobPointer,
                Persist = 3,
                UserName = username
            };
            if (!CredWrite(ref credential, 0))
            {
                throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
            }
        }
        finally
        {
            Array.Clear(blob, 0, blob.Length);
            for (int index = 0; index < blob.Length; index++) Marshal.WriteByte(blobPointer, index, 0);
            Marshal.FreeCoTaskMem(blobPointer);
            Marshal.FreeCoTaskMem(username);
            Marshal.FreeCoTaskMem(target);
        }
    }
}
'@

$secureKey = Import-Clixml -LiteralPath $legacyPath
$pointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secureKey)
try {
    $plainKey = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($pointer)
    [DelegateCredentialWriter]::Write($plainKey)
}
finally {
    $plainKey = $null
    [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($pointer)
}

Write-Output 'CREDENTIAL_MIGRATION_OK'
