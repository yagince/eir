import { Octokit } from '@octokit/core'
import { Octokit as OctokitRest } from '@octokit/rest'

export default class Github {
  token: string;
  client: Octokit;

  constructor (token: string) {
    this.token = token
    this.client = this.buildClient()
  }

  buildClient (): Octokit {
    return new OctokitRest({
      auth: this.token
    })
  }

  async user () {
    const { data } = await this.client.request('/user')
    return data
  }
}
