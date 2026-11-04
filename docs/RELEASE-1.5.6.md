# ZORU Miner 1.5.6

Mining resumes with the current beacon after slow network responses. Beacon requests no longer accumulate while other transactions are being processed.

Existing wallet files, transaction journals and reward history are preserved. The release uses the current ZORU mainnet contracts on chain 6736.

The beacon recovery check used real isolated VEX transactions: a 75-second outage with delayed responses, followed by 15 accepted proofs, seven automatic claims and three mining epochs. Linux application checks passed.

Available for Linux x86_64 as a Debian package and a portable archive.
