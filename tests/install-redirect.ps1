# Checks that install.ps1's Get-Https follows HTTPS redirects and refuses a downgrade to HTTP, bounds
# redirect loops, and refuses a plain HTTP URL; the published launch command's `irm -MaximumRedirection 0`
# fetches without a redirect and refuses any. Run it in Windows PowerShell 5.1, whose WebClient and
# Invoke-WebRequest follow such a downgrade. A local TLS server stands in for the redirecting host;
# -Pfx names a certificate to serve where New-SelfSignedCertificate does not exist.
param([string]$Pfx)
$ErrorActionPreference = 'Stop'

$source = Get-Content -Raw (Join-Path $PSScriptRoot '..\install.ps1')
$function = [regex]::Match($source, '(?s)\tfunction Get-Https.*?\r?\n\t}\r?\n')
if (-not $function.Success) { throw 'tests: install.ps1 has no Get-Https function' }
Invoke-Expression $function.Value

# A scriptblock callback has no runspace on the thread TLS calls it from, so compile one.
Add-Type 'public static class TrustAny { public static readonly System.Net.Security.RemoteCertificateValidationCallback Callback = (s, c, ch, e) => true; }'
[Net.ServicePointManager]::ServerCertificateValidationCallback = [TrustAny]::Callback
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

$cert = if ($Pfx) { New-Object Security.Cryptography.X509Certificates.X509Certificate2 $Pfx } else { New-SelfSignedCertificate -DnsName localhost -CertStoreLocation Cert:\CurrentUser\My }
$listener = New-Object Net.Sockets.TcpListener ([Net.IPAddress]::Loopback), 0
$listener.Start()
$port = $listener.LocalEndpoint.Port

$server = [powershell]::Create().AddScript({
	param($listener, $cert, $port)
	$routes = @{
		'/ok' = '200 OK', $null
		'/follow' = '302 Found', '/ok'
		'/down' = '302 Found', "http://localhost:$port/ok"
		'/loop' = '302 Found', '/loop'
	}
	while ($true) {
		try { $client = $listener.AcceptTcpClient() } catch { break }
		try {
			$stream = New-Object Net.Security.SslStream $client.GetStream(), $false
			$stream.AuthenticateAsServer($cert)
			$head = ''
			while (-not $head.EndsWith("`r`n`r`n")) {
				$byte = $stream.ReadByte()
				if ($byte -lt 0) { break }
				$head += [char]$byte
			}
			$status, $location = $routes[($head -split ' ')[1]]
			$body = if ($location) { '' } else { 'ok' }
			$lines = "HTTP/1.1 $status", "Content-Length: $($body.Length)", 'Connection: close'
			if ($location) { $lines += "Location: $location" }
			$bytes = [Text.Encoding]::ASCII.GetBytes(($lines -join "`r`n") + "`r`n`r`n" + $body)
			$stream.Write($bytes, 0, $bytes.Length)
			$stream.Flush()
		} catch {
		} finally {
			$client.Close()
		}
	}
}).AddArgument($listener).AddArgument($cert).AddArgument($port)
$pending = $server.BeginInvoke()

function Expect-Failure([string]$path, [string]$pattern) {
	try {
		Get-Https $path "$file.failed"
	} catch {
		if ("$_" -notmatch $pattern) { throw "tests: $path failed with '$_', not /$pattern/" }
		return
	}
	throw "tests: $path was fetched, not refused"
}

$file = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
try {
	Get-Https "https://localhost:$port/follow" $file
	if ((Get-Content -Raw $file) -ne 'ok') { throw 'tests: an HTTPS redirect was not followed' }
	Expect-Failure "https://localhost:$port/down" 'non-HTTPS'
	Expect-Failure "http://localhost:$port/ok" 'non-HTTPS'
	Expect-Failure "https://localhost:$port/loop" 'too many redirects'
	if ((irm -MaximumRedirection 0 "https://localhost:$port/ok") -ne 'ok') { throw 'tests: irm -MaximumRedirection 0 did not fetch' }
	foreach ($path in '/follow', '/down') {
		try { irm -MaximumRedirection 0 "https://localhost:$port$path" | Out-Null } catch { continue }
		throw "tests: irm -MaximumRedirection 0 followed $path"
	}
	Write-Host 'Get-Https follows HTTPS redirects and refuses HTTP ones'
} finally {
	$listener.Stop()
	$server.Dispose()
	Remove-Item -Force -ErrorAction SilentlyContinue $file, "$file.failed"
}
