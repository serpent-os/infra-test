<script lang="ts">
	import type { Endpoint } from '$lib/api';

	let { endpoint } = $props<{ endpoint: Endpoint }>();
</script>

<div class="card">
	<div class="title">{endpoint.id}</div>
	<div class="body">
		<div>{endpoint.host_address}</div>
		<div>{endpoint.status}</div>
	</div>
</div>

<style>
	.card {
		flex: 0 1 auto;
		padding: 0.4em;
		border: 1px solid;
		border-radius: 4px;
	}

	.body {
		display: flex;
		flex-flow: column nowrap;
		gap: 4px;
	}

	.title {
		font-weight: bold;
		margin-bottom: 6px;
	}
</style>
