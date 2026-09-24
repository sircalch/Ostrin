(function () {
  const SITE_FACTS = Object.freeze(globalThis.OSTRIN_SITE_FACTS || {});

  const released = SITE_FACTS.releaseStatus === 'published';
  document.querySelectorAll('.version').forEach(function (label) {
    label.textContent = released ? 'v' + SITE_FACTS.version + ' / experimental' : 'development / ' + SITE_FACTS.version;
  });

  // The release line only claims what scripts/site-facts.mjs derived from CHANGELOG.md.
  document.querySelectorAll('[data-release-line]').forEach(function (line) {
    const text = line.querySelector('[data-release-text]');
    if (!text) return;
    line.dataset.state = released ? 'published' : 'unreleased';
    if (released) {
      text.textContent = 'Ostrin v' + SITE_FACTS.version + ' · experimental developer release · published ' + SITE_FACTS.releaseDate + ' · ';
      const link = document.createElement('a');
      link.href = SITE_FACTS.releaseUrl;
      link.textContent = 'release notes ↗';
      text.append(link);
    } else {
      text.textContent = 'Version ' + SITE_FACTS.version + ' is in development; no release has been published yet.';
    }
  });

  document.querySelectorAll('[data-site-value]').forEach(function (node) {
    const value = SITE_FACTS[node.dataset.siteValue];
    if (value !== undefined) node.textContent = value;
  });

  document.querySelectorAll('.site-footer > span').forEach(function (label) {
    label.textContent = 'Open development · MIT License';
  });

  const header = document.querySelector('.site-header');
  const menuButton = document.querySelector('.menu-toggle');

  if (header && menuButton) {
    menuButton.addEventListener('click', function () {
      const active = header.classList.toggle('menu-active');
      document.body.classList.toggle('menu-open', active);
      menuButton.setAttribute('aria-expanded', String(active));
    });
  }

  document.querySelectorAll('.site-nav a').forEach(function (link) {
    link.addEventListener('click', function () {
      if (header) header.classList.remove('menu-active');
      document.body.classList.remove('menu-open');
    });
  });

  document.querySelectorAll('.filter-button').forEach(function (button) {
    button.addEventListener('click', function () {
      const filter = button.dataset.filter;
      document.querySelectorAll('.filter-button').forEach(function (item) {
        item.classList.toggle('active', item === button);
      });
      document.querySelectorAll('.example-card').forEach(function (card) {
        card.classList.toggle('is-hidden', filter !== 'all' && card.dataset.category !== filter);
      });
    });
  });

  document.querySelectorAll('.tab-button').forEach(function (button) {
    button.addEventListener('click', function () {
      const target = button.dataset.tab;
      document.querySelectorAll('.tab-button').forEach(function (item) {
        item.classList.toggle('active', item === button);
      });
      document.querySelectorAll('.tab-panel').forEach(function (panel) {
        panel.classList.toggle('active', panel.id === target);
      });
    });
  });

  document.querySelectorAll('.copy-button').forEach(function (button) {
    button.addEventListener('click', async function () {
      const target = document.getElementById(button.dataset.copy);
      if (!target || !navigator.clipboard) return;
      try {
        await navigator.clipboard.writeText(target.innerText);
        const label = button.textContent;
        button.textContent = 'copied';
        setTimeout(function () { button.textContent = label; }, 1200);
      } catch (_) {
        button.textContent = 'select code';
      }
    });
  });
})();
