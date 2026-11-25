# Install crabx's release binary:
#   powershell -ExecutionPolicy Bypass -c "[Net.ServicePointManager]::SecurityProtocol = 'Tls12'; irm -MaximumRedirection 0 -ErrorAction Stop https://crabx.kixel.me/install.ps1 | iex"
# CRABX_VERSION picks a version instead of the newest stable one on crates.io, and
# CRABX_INSTALL_DIR the directory instead of ~\.local\bin.
# A script block keeps its variables out of the session `iex` runs it in, and errors throw
# rather than exit, which would close that session.
& {
	$ErrorActionPreference = 'Stop'
	$ProgressPreference = 'SilentlyContinue'
	# Windows PowerShell 5.1 may not offer TLS 1.2 by default.
	[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12

	# Saves a URL to a file, following up to five redirects and refusing any hop that is not HTTPS:
	# Windows PowerShell 5.1's WebClient and Invoke-WebRequest follow an HTTPS-to-HTTP redirect.
	function Get-Https([string]$uri, [string]$file) {
		for ($hop = 0; $hop -le 5; $hop++) {
			if (([Uri]$uri).Scheme -ne 'https') { throw "crabx install: refusing to fetch $uri over a non-HTTPS connection" }
			$request = [Net.HttpWebRequest]::Create($uri)
			$request.AllowAutoRedirect = $false
			$response = $request.GetResponse()
			try {
				$status = [int]$response.StatusCode
				if ($status -ge 300 -and $status -lt 400) {
					$location = $response.Headers['Location']
					if (-not $location) { throw "crabx install: $uri redirected with no Location" }
					$uri = [Uri]::new([Uri]$uri, $location).AbsoluteUri
					continue
				}
				$out = [IO.File]::Create($file)
				try { $response.GetResponseStream().CopyTo($out) } finally { $out.Dispose() }
				return
			} finally {
				$response.Dispose()
			}
		}
		throw "crabx install: too many redirects fetching $uri"
	}

	# The OS's architecture, not this process's: an x64 PowerShell on Arm reports x64.
	$arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture
	$target = switch ("$arch") {
		'X64' { 'x86_64-pc-windows-msvc' }
		'Arm64' { 'aarch64-pc-windows-msvc' }
		default { throw "crabx install: no prebuilt crabx for '$arch'; install it with ``cargo install crabx``" }
	}

	$temp = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
	New-Item -ItemType Directory $temp | Out-Null
	try {
		$version = $env:CRABX_VERSION
		if (-not $version) {
			# The highest version on crates.io that is neither yanked nor a prerelease.
			try {
				Get-Https 'https://index.crates.io/cr/ab/crabx' "$temp\index"
			} catch {
				throw "crabx install: cannot read crabx's versions from crates.io: $_"
			}
			$version = Get-Content "$temp\index" | Where-Object { $_ } | ForEach-Object { $_ | ConvertFrom-Json } |
				Where-Object { -not $_.yanked -and ($_.vers -replace '\+.*', '') -notmatch '-' } |
				Sort-Object { [version]($_.vers -replace '\+.*', '') } | Select-Object -Last 1 -ExpandProperty vers
			if (-not $version) { throw 'crabx install: crates.io has no stable crabx release' }
		}
		# SemVer 2.0.0: no leading zeros in numeric parts, no empty identifiers. \z, not $, which
		# would let a trailing newline through.
		$num = '(0|[1-9][0-9]*)'
		$pre = '(0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)'
		$build = '[0-9A-Za-z-]+'
		if ($version -notmatch "^$num\.$num\.$num(-$pre(\.$pre)*)?(\+$build(\.$build)*)?\z") { throw "crabx install: invalid version $version; expected MAJOR.MINOR.PATCH as SemVer 2.0.0 defines it" }

		$dir = if ($env:CRABX_INSTALL_DIR) { $env:CRABX_INSTALL_DIR } else { Join-Path $HOME '.local\bin' }
		$name = "crabx-$target-v$version"
		$url = "https://github.com/kixel-ai/crabx/releases/download/crabx-v$version/$name.tgz"
		Write-Host "crabx install: downloading crabx $version for $target"
		try {
			Get-Https $url "$temp\crabx.tgz"
		} catch {
			throw "crabx install: cannot download ${url}; crabx $version's binaries may not be released yet: $_"
		}
		# Windows 10 1803 and later ship bsdtar.
		tar -xzf "$temp\crabx.tgz" -C $temp "$name/crabx.exe"
		if ($LASTEXITCODE -ne 0) { throw "crabx install: cannot extract $name.tgz" }
		New-Item -ItemType Directory -Force $dir | Out-Null
		Move-Item -Force "$temp\$name\crabx.exe" (Join-Path $dir 'crabx.exe')
	} finally {
		Remove-Item -Recurse -Force $temp
	}
	Write-Host "crabx install: installed $(Join-Path $dir 'crabx.exe')"

	# The registry's raw value keeps entries such as %JAVA_HOME%\bin expandable, and setting a user
	# variable through .NET tells running programs, such as Explorer, that the environment changed.
	$key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
	$path = $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
	if (($path -split ';') -notcontains $dir) {
		$kind = if ($path) { $key.GetValueKind('Path') } else { [Microsoft.Win32.RegistryValueKind]::ExpandString }
		$key.SetValue('Path', ((@($path, $dir) | Where-Object { $_ }) -join ';'), $kind)
		[Environment]::SetEnvironmentVariable('CRABX_PATH_CHANGED', '1', 'User')
		[Environment]::SetEnvironmentVariable('CRABX_PATH_CHANGED', $null, 'User')
		$env:Path = "$env:Path;$dir"
		Write-Host "crabx install: added $dir to your PATH; new terminals will find crabx"
	}
	$key.Close()

	# Relink crate launchers to the new crabx.exe; 0.2.0 is the first version with `crabx install`.
	if ([version]($version -replace '[-+].*', '') -ge [version]'0.2.0') {
		& (Join-Path $dir 'crabx.exe') install
		if ($LASTEXITCODE -ne 0) { throw "crabx install: crabx install failed with exit code $LASTEXITCODE" }
	}
}
