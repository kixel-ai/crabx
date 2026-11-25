# Checks that install.ps1's Get-Https follows HTTPS redirects and refuses a downgrade to HTTP, bounds
# redirect loops, and refuses a plain HTTP URL; and that the published launch command's
# `curl.exe --proto '=https' --tlsv1.2 -fsSL | Out-String | iex` runs what an HTTPS redirect leads to but
# neither runs the body of a redirect to HTTP nor anything it leaves. Run it in Windows PowerShell 5.1,
# whose WebClient and Invoke-WebRequest follow an HTTPS-to-HTTP redirect. A local TLS server stands in for the redirecting host; -Pfx names a
# certificate to serve where New-SelfSignedCertificate does not exist.
param([string]$Pfx)
$ErrorActionPreference = 'Stop'

$source = Get-Content -Raw (Join-Path $PSScriptRoot '..\install.ps1')
$function = [regex]::Match($source, '(?s)\tfunction Get-Https.*?\r?\n\t}\r?\n')
if (-not $function.Success) { throw 'tests: install.ps1 has no Get-Https function' }
Invoke-Expression $function.Value

# A scriptblock callback has no runspace on the thread TLS calls it from, so compile one.
Add-Type 'public static class TrustAny { public static readonly System.Net.Security.RemoteCertificateValidationCallback Callback = (s, c, ch, e) => true; }'

function Expect-Failure([string]$uri, [string]$pattern) {
	try {
		Get-Https $uri "$file.failed"
	} catch {
		if ("$_" -notmatch $pattern) { throw "tests: $uri failed with '$_', not /$pattern/" }
		return
	}
	throw "tests: $uri was fetched, not refused"
}

# The session outlives this script, so leave its TLS settings as they were.
$validation = [Net.ServicePointManager]::ServerCertificateValidationCallback
$protocol = [Net.ServicePointManager]::SecurityProtocol
$errorAction = $ErrorActionPreference
$listener = $server = $null
$file = Join-Path ([IO.Path]::GetTempPath()) ([IO.Path]::GetRandomFileName())
try {
	[Net.ServicePointManager]::ServerCertificateValidationCallback = [TrustAny]::Callback
	[Net.ServicePointManager]::SecurityProtocol = $protocol -bor [Net.SecurityProtocolType]::Tls12
	$cert = if ($Pfx) { New-Object Security.Cryptography.X509Certificates.X509Certificate2 $Pfx } else { New-SelfSignedCertificate -DnsName localhost -CertStoreLocation Cert:\CurrentUser\My }
	$listener = New-Object Net.Sockets.TcpListener ([Net.IPAddress]::Loopback), 0
	$listener.Start()
	$port = $listener.LocalEndpoint.Port

	$server = [powershell]::Create().AddScript({
		param($listener, $cert, $port)
		# A redirect's body would run if a launcher piped it to iex.
		$ran = '$global:redirectBodyRan = $true'
		$fetched = '$global:fetched = $true'
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
				$body = if ($location) { $ran } else { $fetched }
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

	Get-Https "https://localhost:$port/follow" $file
	if ((Get-Content -Raw $file) -ne '$global:fetched = $true') { throw 'tests: an HTTPS redirect was not followed' }
	Expect-Failure "https://localhost:$port/down" 'non-HTTPS'
	Expect-Failure "http://localhost:$port/ok" 'non-HTTPS'
	Expect-Failure "https://localhost:$port/loop" 'too many redirects'

	# PowerShell 7's Invoke-RestMethod ignores the callback above.
	$trust = if ($PSVersionTable.PSVersion.Major -ge 6) { @{ SkipCertificateCheck = $true } } else { @{} }
	# The published command, but for -k, since the server's certificate is self-signed. Windows
	# PowerShell's curl is an alias for Invoke-WebRequest, so it names curl.exe; elsewhere it is curl.
	$curl = if ($IsWindows -eq $false) { 'curl' } else { 'curl.exe' }
	$ErrorActionPreference = 'Continue'
	foreach ($case in @(('/ok', $true), ('/follow', $true), ('/down', $false), ('/loop', $false))) {
		$global:fetched = $global:redirectBodyRan = $false
		try { & $curl --proto '=https' --tlsv1.2 -fsSLk "https://localhost:$port$($case[0])" | Out-String | iex } catch { }
		if ($global:redirectBodyRan) { throw "tests: the published command ran the body of the redirect from $($case[0])" }
		if ($global:fetched -ne $case[1]) { throw "tests: the published command fetched=$global:fetched from $($case[0]), not $($case[1])" }
	}
	$ErrorActionPreference = $errorAction
	Write-Host 'Get-Https follows HTTPS redirects and refuses HTTP ones'
} finally {
	$ErrorActionPreference = $errorAction
	[Net.ServicePointManager]::ServerCertificateValidationCallback = $validation
	[Net.ServicePointManager]::SecurityProtocol = $protocol
	if ($listener) { $listener.Stop() }
	if ($server) { $server.Dispose() }
	Remove-Item -Force -ErrorAction SilentlyContinue $file, "$file.failed"
}
