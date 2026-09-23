(function () {
  const SITE_FACTS = Object.freeze({
    version: '0.1.0',
    designDocs: '22',
    examples: '195',
    integrationTests: '195',
    differentialTests: '6',
    unitTests: '2',
  });

  document.querySelectorAll('.version').forEach(function (label) {
    label.textContent = 'development / ' + SITE_FACTS.version;
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

  const nav = document.querySelector('.site-nav');
  const githubLink = nav && nav.querySelector('a[href^="https://github.com"]');
  const ecosystemLink = nav && nav.querySelector('a[href="ecosystem.html"]');
  if (nav && githubLink) {
    [['showcase.html', 'Showcase'], ['community.html', 'Community']].forEach(function (item) {
      if (!nav.querySelector('a[href="' + item[0] + '"]')) {
        const link = document.createElement('a');
        link.href = item[0];
        link.textContent = item[1];
        link.addEventListener('click', function () {
          if (header) header.classList.remove('menu-active');
          document.body.classList.remove('menu-open');
        });
        nav.insertBefore(link, ecosystemLink || githubLink);
      }
    });
  }

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
