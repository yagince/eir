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

  async repos () {
    const repos = await this.client.paginate(this.client.repos.list.endpoint())
    return repos
  }

  async pullRequests () {
    const pulls: { [key: string]: { repo: {[key: string]: string}; pulls: {[key: string]: string}[] } } = {}

    for await (const res of this.client.paginate.iterator(this.client.repos.list.endpoint())) {
      for (const repo of res.data) {
        const { data: repoPulls } = await this.client.pulls.list({ owner: repo.owner.login, repo: repo.name })
        pulls[repo.full_name] = {
          repo: repo,
          pulls: repoPulls
        }
      }
    }

    return pulls
  }
}
