# reseda-mesh
[![Build Docker image and Push to Docker Hub.](https://github.com/bennjii/reseda-mesh/actions/workflows/docker.yml/badge.svg?branch=master)](https://github.com/bennjii/reseda-mesh/actions/workflows/docker.yml)

The reseda mesh aims to act as a centralized node for the reseda vpn network, to manage and maintain the mesh. 
This module acts to allow and pass verification of sub-servers running the [`reseda-rust`](http://github.com/bennjii/reseda-rust) framework. 
It maintains the subnodes, monitoring their status and organizing them statefully for their public appearance on the reseda database.

> This module is not intended for client deployment.
